use super::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct StoredChunkMeta {
    pub bounds: ChunkBounds,
    pub metrics: ChunkMetrics,
    pub neighbors: ChunkNeighbors,
}

impl StoredChunkMeta {
    pub(crate) fn from_chunk_meta(meta: &ChunkMeta) -> Self {
        Self {
            bounds: meta.bounds,
            metrics: meta.metrics,
            neighbors: meta.neighbors,
        }
    }

    pub(crate) fn into_chunk_meta(
        self,
        key: ChunkKey,
        surface_class: SurfaceClassKey,
    ) -> Result<ChunkMeta, TopologyError> {
        Ok(ChunkMeta {
            key,
            bounds: self.bounds,
            metrics: self.metrics,
            neighbors: self.neighbors,
            surface_class,
        })
    }
}

#[derive(Clone, Debug)]
pub struct MetadataStore {
    dense_max_lod: u8,
    dense_levels: Vec<[Vec<Option<StoredChunkMeta>>; 6]>,
    sparse_levels: HashMap<ChunkKey, StoredChunkMeta>,
    len: usize,
}

impl MetadataStore {
    pub fn new(runtime_max_lod: u8, dense_max_lod: u8) -> Self {
        let dense_max_lod = dense_max_lod.min(runtime_max_lod);
        let level_count = usize::from(dense_max_lod) + 1;
        let dense_levels = (0..level_count)
            .map(|_| std::array::from_fn(|_| Vec::new()))
            .collect();
        Self {
            dense_max_lod,
            dense_levels,
            sparse_levels: HashMap::new(),
            len: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn dense_max_lod(&self) -> u8 {
        self.dense_max_lod
    }

    pub fn sparse_count(&self) -> usize {
        self.sparse_levels.len()
    }

    pub fn level_is_built(&self, lod: u8) -> bool {
        if !self.is_dense_lod(lod) {
            return false;
        }

        self.dense_levels
            .get(usize::from(lod))
            .map(|faces| faces.iter().all(|entries| !entries.is_empty()))
            .unwrap_or(false)
    }

    pub fn contains_key(&self, key: &ChunkKey) -> bool {
        self.get_stored(key).is_some()
    }

    pub fn center_planet(&self, key: &ChunkKey) -> Option<DVec3> {
        self.get_stored(key)
            .map(|stored| stored.bounds.center_planet)
    }

    pub fn neighbors(&self, key: &ChunkKey) -> Option<ChunkNeighbors> {
        self.get_stored(key).map(|stored| stored.neighbors)
    }

    pub fn get_chunk_meta(
        &self,
        key: ChunkKey,
        surface_class: SurfaceClassKey,
    ) -> Result<Option<ChunkMeta>, TopologyError> {
        self.get_stored(&key)
            .map(|stored| stored.into_chunk_meta(key, surface_class))
            .transpose()
    }

    pub fn insert_chunk_meta(
        &mut self,
        meta: ChunkMeta,
        prevalidated_neighbors: bool,
    ) -> Result<Option<ChunkMeta>, TopologyError> {
        let mut meta = meta;
        if !prevalidated_neighbors {
            meta.refresh_same_lod_neighbors()?;
        }

        if self.is_dense_lod(meta.key.lod) {
            let (lod_index, face_index, slot_index, slot_count) =
                self.slot_components(&meta.key)
                    .ok_or(TopologyError::InvalidChunkKey)?;
            let face_slots = &mut self.dense_levels[lod_index][face_index];
            if face_slots.is_empty() {
                face_slots.resize(slot_count, None);
            }

            let previous = face_slots[slot_index]
                .replace(StoredChunkMeta::from_chunk_meta(&meta))
                .map(|stored| stored.into_chunk_meta(meta.key, meta.surface_class.clone()))
                .transpose()?;
            if previous.is_none() {
                self.len += 1;
            }
            Ok(previous)
        } else {
            let previous = self
                .sparse_levels
                .insert(meta.key, StoredChunkMeta::from_chunk_meta(&meta))
                .map(|stored| stored.into_chunk_meta(meta.key, meta.surface_class.clone()))
                .transpose()?;
            if previous.is_none() {
                self.len += 1;
            }
            Ok(previous)
        }
    }

    pub(crate) fn set_face_level(
        &mut self,
        face: Face,
        lod: u8,
        entries: Vec<StoredChunkMeta>,
    ) -> Result<(), TopologyError> {
        if !self.is_dense_lod(lod) {
            return Err(TopologyError::InvalidChunkKey);
        }

        let slot_count = self.face_slot_count(lod);
        if entries.len() != slot_count {
            return Err(TopologyError::InvalidChunkKey);
        }

        let lod_index = usize::from(lod);
        let face_index = Self::face_index(face);
        let previous_count = self.dense_levels[lod_index][face_index]
            .iter()
            .filter(|entry| entry.is_some())
            .count();
        self.len = self.len.saturating_sub(previous_count);
        self.len += entries.len();
        self.dense_levels[lod_index][face_index] = entries.into_iter().map(Some).collect();
        Ok(())
    }

    fn get_stored(&self, key: &ChunkKey) -> Option<&StoredChunkMeta> {
        if self.is_dense_lod(key.lod) {
            let (lod_index, face_index, slot_index, _) = self.slot_components(key)?;
            self.dense_levels
                .get(lod_index)?
                .get(face_index)?
                .get(slot_index)?
                .as_ref()
        } else {
            self.sparse_levels.get(key)
        }
    }

    fn is_dense_lod(&self, lod: u8) -> bool {
        lod <= self.dense_max_lod
    }

    fn slot_components(&self, key: &ChunkKey) -> Option<(usize, usize, usize, usize)> {
        if !key.is_valid_for_lod() {
            return None;
        }
        let lod_index = usize::from(key.lod);
        let face_index = Self::face_index(key.face);
        let resolution = ChunkKey::resolution_for_lod(key.lod) as usize;
        let slot_index = key.y as usize * resolution + key.x as usize;
        Some((lod_index, face_index, slot_index, resolution * resolution))
    }

    fn face_slot_count(&self, lod: u8) -> usize {
        let resolution = ChunkKey::resolution_for_lod(lod) as usize;
        resolution * resolution
    }

    fn face_index(face: Face) -> usize {
        match face {
            Face::Px => 0,
            Face::Nx => 1,
            Face::Py => 2,
            Face::Ny => 3,
            Face::Pz => 4,
            Face::Nz => 5,
        }
    }
}
