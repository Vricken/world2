use super::*;

pub const DEFAULT_PLANET_SEED: u64 = 0x5EED_5EED_D15C_A11E;
pub const DEFAULT_ASSET_PLACEMENT_CELLS_PER_AXIS: u32 = 8;
pub const DEFAULT_ASSET_GROUP_CHUNK_SPAN: u32 = 2;
pub const DEFAULT_ASSET_MASK_TEXTURE_NOTE: &str = "procedural_mask_signal";

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AssetFamilyDefinition {
    pub family_id: u16,
    pub spawn_chance: f32,
    pub min_biome0: f32,
    pub min_biome1: f32,
    pub max_slope_hint: f32,
    pub min_altitude_norm: f32,
    pub max_altitude_norm: f32,
    pub max_curvature_hint: f32,
    pub min_mask_value: f32,
    pub exclusion_radius: f64,
    pub base_scale: f32,
    pub scale_jitter: f32,
    pub mesh_radius: f32,
    pub mesh_height: f32,
}

impl AssetFamilyDefinition {
    pub const fn mesh_half_extents(self) -> Vector3 {
        Vector3::new(self.mesh_radius, self.mesh_height * 0.5, self.mesh_radius)
    }
}

pub const ASSET_FAMILY_DEFINITIONS: [AssetFamilyDefinition; 2] = [
    AssetFamilyDefinition {
        family_id: 0,
        spawn_chance: 0.34,
        min_biome0: 0.38,
        min_biome1: 0.28,
        max_slope_hint: 0.16,
        min_altitude_norm: 0.28,
        max_altitude_norm: 0.74,
        max_curvature_hint: 0.12,
        min_mask_value: 0.42,
        exclusion_radius: 20.0,
        base_scale: 5.0,
        scale_jitter: 1.3,
        mesh_radius: 0.65,
        mesh_height: 3.4,
    },
    AssetFamilyDefinition {
        family_id: 1,
        spawn_chance: 0.18,
        min_biome0: 0.18,
        min_biome1: 0.34,
        max_slope_hint: 0.28,
        min_altitude_norm: 0.45,
        max_altitude_norm: 0.92,
        max_curvature_hint: 0.18,
        min_mask_value: 0.56,
        exclusion_radius: 34.0,
        base_scale: 8.5,
        scale_jitter: 2.1,
        mesh_radius: 0.8,
        mesh_height: 5.4,
    },
];

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChunkAssetPlacement {
    pub assets: Vec<AssetInstance>,
    pub candidate_count: usize,
    pub rejected_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AssetGroupKey {
    pub face: Face,
    pub lod: u8,
    pub group_x: u32,
    pub group_y: u32,
    pub family_id: u16,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DesiredAssetGroup {
    pub key: AssetGroupKey,
    pub group_origin_planet: DVec3,
    pub source_chunks: Vec<ChunkKey>,
    pub assets: Vec<AssetInstance>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AssetGroupState {
    pub key: AssetGroupKey,
    pub group_origin_planet: DVec3,
    pub source_chunks: Vec<ChunkKey>,
    pub instance_count: usize,
    pub multimesh_rid: Option<Rid>,
    pub render_instance_rid: Option<Rid>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AssetDebugSnapshot {
    pub active_groups: usize,
    pub active_instances: usize,
    pub family_meshes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct AssetSiteSample {
    displaced_point: DVec3,
    normal: DVec3,
    biome0: f32,
    biome1: f32,
    slope_hint: f32,
    curvature_hint: f32,
    altitude_norm: f32,
    mask_value: f32,
}

pub fn build_chunk_asset_placement(config: &RuntimeConfig, key: ChunkKey) -> ChunkAssetPlacement {
    let cells_per_axis = config.asset_placement_cells_per_axis.max(1);
    let cell_extent = 1.0 / f64::from(cells_per_axis);
    let mut placement = ChunkAssetPlacement::default();
    let terrain = terrain_settings(config);

    for family in ASSET_FAMILY_DEFINITIONS {
        for cell_y in 0..cells_per_axis {
            for cell_x in 0..cells_per_axis {
                placement.candidate_count += 1;
                let cell_id = cell_y * cells_per_axis + cell_x;
                let base_hash = placement_hash(config.planet_seed, key, cell_id, family.family_id);
                if unit_f32(base_hash ^ 0xA135_2F31_94C1_0E57) > family.spawn_chance {
                    placement.rejected_count += 1;
                    continue;
                }

                let chunk_uv = DVec2::new(
                    (f64::from(cell_x) + unit_f64(base_hash ^ 0x9E37_79B9_7F4A_7C15)) * cell_extent,
                    (f64::from(cell_y) + unit_f64(base_hash ^ 0xC6BC_2796_92B5_CC83)) * cell_extent,
                );
                let Some(site) = sample_asset_site(config, key, chunk_uv, &terrain) else {
                    placement.rejected_count += 1;
                    continue;
                };

                let accepted = site.biome0 >= family.min_biome0
                    && site.biome1 >= family.min_biome1
                    && site.slope_hint <= family.max_slope_hint
                    && site.altitude_norm >= family.min_altitude_norm
                    && site.altitude_norm <= family.max_altitude_norm
                    && site.curvature_hint <= family.max_curvature_hint
                    && site.mask_value >= family.min_mask_value
                    && respects_exclusion_radius(
                        &placement.assets,
                        family.family_id,
                        site.displaced_point,
                        family.exclusion_radius,
                    );

                if !accepted {
                    placement.rejected_count += 1;
                    continue;
                }

                placement
                    .assets
                    .push(build_asset_instance(base_hash, family, site));
            }
        }
    }

    placement
}

pub fn asset_group_key_for_chunk(key: ChunkKey, family_id: u16, chunk_span: u32) -> AssetGroupKey {
    let chunk_span = chunk_span.max(1);
    AssetGroupKey {
        face: key.face,
        lod: key.lod,
        group_x: key.x / chunk_span,
        group_y: key.y / chunk_span,
        family_id,
    }
}

pub fn asset_group_anchor_key(key: AssetGroupKey, chunk_span: u32) -> ChunkKey {
    let chunk_span = chunk_span.max(1);
    let resolution = ChunkKey::resolution_for_lod(key.lod).max(1);
    let x = key.group_x.saturating_mul(chunk_span).min(resolution - 1);
    let y = key.group_y.saturating_mul(chunk_span).min(resolution - 1);
    ChunkKey::new(key.face, key.lod, x, y)
}

pub fn build_desired_asset_groups(
    config: &RuntimeConfig,
    active_render: &HashSet<ChunkKey>,
    resident_payloads: &HashMap<ChunkKey, ChunkPayload>,
    meta: &HashMap<ChunkKey, ChunkMeta>,
) -> HashMap<AssetGroupKey, DesiredAssetGroup> {
    let mut groups = HashMap::new();

    for key in active_render.iter().copied() {
        let Some(payload) = resident_payloads.get(&key) else {
            continue;
        };
        for asset in &payload.assets {
            let group_key =
                asset_group_key_for_chunk(key, asset.family_id, config.asset_group_chunk_span);
            let entry = groups.entry(group_key).or_insert_with(|| {
                let anchor_key = asset_group_anchor_key(group_key, config.asset_group_chunk_span);
                let group_origin_planet = meta
                    .get(&anchor_key)
                    .map(|meta| meta.bounds.center_planet)
                    .or_else(|| meta.get(&key).map(|meta| meta.bounds.center_planet))
                    .unwrap_or(asset.origin);
                DesiredAssetGroup {
                    key: group_key,
                    group_origin_planet,
                    source_chunks: Vec::new(),
                    assets: Vec::new(),
                }
            });
            if entry.source_chunks.last().copied() != Some(key) {
                entry.source_chunks.push(key);
            }
            entry.assets.push(asset.clone());
        }
    }

    for group in groups.values_mut() {
        group.source_chunks.sort_unstable();
    }

    groups
}

pub fn asset_debug_snapshot(
    asset_groups: &HashMap<AssetGroupKey, AssetGroupState>,
    family_meshes: &HashMap<u16, Rid>,
) -> AssetDebugSnapshot {
    AssetDebugSnapshot {
        active_groups: asset_groups.len(),
        active_instances: asset_groups
            .values()
            .map(|state| state.instance_count)
            .sum(),
        family_meshes: family_meshes.len(),
    }
}

pub fn build_asset_mesh(surface: AssetFamilyDefinition) -> CpuMeshBuffers {
    let radius = surface.mesh_radius;
    let height = surface.mesh_height;
    let half_height = height * 0.5;
    let positions = [
        [0.0, half_height, 0.0],
        [-radius, -half_height, -radius],
        [radius, -half_height, -radius],
        [radius, -half_height, radius],
        [-radius, -half_height, radius],
    ];
    let triangles = [
        (0, 1, 2),
        (0, 2, 3),
        (0, 3, 4),
        (0, 4, 1),
        (1, 4, 3),
        (1, 3, 2),
    ];
    let mut mesh = CpuMeshBuffers::default();

    for (a, b, c) in triangles {
        let a = positions[a];
        let b = positions[b];
        let c = positions[c];
        let ab = Vector3::new(b[0] - a[0], b[1] - a[1], b[2] - a[2]);
        let ac = Vector3::new(c[0] - a[0], c[1] - a[1], c[2] - a[2]);
        let normal = ab.cross(ac).normalized();
        let tangent = Vector3::new(1.0, 0.0, 0.0);
        let base = mesh.positions.len() as i32;

        for position in [a, b, c] {
            mesh.positions.push(position);
            mesh.normals.push([normal.x, normal.y, normal.z]);
            mesh.tangents.push([tangent.x, tangent.y, tangent.z, 1.0]);
            mesh.uvs.push([
                position[0] / (radius * 2.0) + 0.5,
                position[2] / (radius * 2.0) + 0.5,
            ]);
            mesh.colors.push([1.0, 1.0, 1.0, 1.0]);
        }
        mesh.indices.extend_from_slice(&[base, base + 1, base + 2]);
    }

    mesh
}

pub fn asset_group_local_aabb(
    group_origin_planet: DVec3,
    assets: &[AssetInstance],
) -> Option<Aabb> {
    let mut min = Vector3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY);
    let mut max = Vector3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);

    for asset in assets {
        let family = ASSET_FAMILY_DEFINITIONS
            .iter()
            .copied()
            .find(|family| family.family_id == asset.family_id)?;
        let center = dvec3_to_vector3(asset.origin - group_origin_planet);
        let extents = family.mesh_half_extents() * asset.scale.max(0.01);
        let asset_min = center - extents;
        let asset_max = center + extents;
        min = Vector3::new(
            min.x.min(asset_min.x),
            min.y.min(asset_min.y),
            min.z.min(asset_min.z),
        );
        max = Vector3::new(
            max.x.max(asset_max.x),
            max.y.max(asset_max.y),
            max.z.max(asset_max.z),
        );
    }

    (min.x.is_finite() && min.y.is_finite() && min.z.is_finite()).then(|| Aabb::new(min, max - min))
}

fn terrain_settings(config: &RuntimeConfig) -> TerrainFieldSettings {
    TerrainFieldSettings {
        planet_radius: config.planet_radius,
        height_amplitude: config.height_amplitude,
        ..TerrainFieldSettings::default()
    }
}

fn sample_asset_site(
    config: &RuntimeConfig,
    key: ChunkKey,
    chunk_uv: DVec2,
    terrain: &TerrainFieldSettings,
) -> Option<AssetSiteSample> {
    let sample_offset = (0.35 / f64::from(config.asset_placement_cells_per_axis.max(1))).min(0.02);
    let center = sample_surface_point(config, key, chunk_uv, terrain)?;
    let left = sample_surface_point(
        config,
        key,
        DVec2::new((chunk_uv.x - sample_offset).clamp(0.0, 1.0), chunk_uv.y),
        terrain,
    )?;
    let right = sample_surface_point(
        config,
        key,
        DVec2::new((chunk_uv.x + sample_offset).clamp(0.0, 1.0), chunk_uv.y),
        terrain,
    )?;
    let down = sample_surface_point(
        config,
        key,
        DVec2::new(chunk_uv.x, (chunk_uv.y - sample_offset).clamp(0.0, 1.0)),
        terrain,
    )?;
    let up = sample_surface_point(
        config,
        key,
        DVec2::new(chunk_uv.x, (chunk_uv.y + sample_offset).clamp(0.0, 1.0)),
        terrain,
    )?;

    let tangent_u = (right.displaced_point - left.displaced_point).normalize_or_zero();
    let tangent_v = (up.displaced_point - down.displaced_point).normalize_or_zero();
    let normal = tangent_u.cross(tangent_v).normalize_or_zero();
    let gradient =
        ((right.height - left.height).powi(2) + (up.height - down.height).powi(2)).sqrt();
    let amplitude = config.height_amplitude.max(f64::from(f32::EPSILON));
    let slope_hint = (gradient / (amplitude * 2.0)).clamp(0.0, 1.0) as f32;
    let curvature =
        (left.height + right.height + down.height + up.height - 4.0 * center.height).abs();
    let curvature_hint = (curvature / (amplitude * 4.0)).clamp(0.0, 1.0) as f32;
    let altitude_norm = (((center.height / amplitude) * 0.5 + 0.5).clamp(0.0, 1.0)) as f32;
    let temperature = (1.0 - center.unit_dir.y.abs()) as f32;
    let moisture_signal =
        (center.unit_dir.dot(DVec3::new(1.731, -0.613, 0.947)).sin() * 0.5 + 0.5) as f32;
    let biome0 = moisture_signal.clamp(0.0, 1.0);
    let biome1 = ((temperature * 0.75) + ((((center.height / amplitude) * 0.25) + 0.25) as f32))
        .clamp(0.0, 1.0);
    let mask_value =
        ((center.unit_dir.dot(DVec3::new(-0.914, 0.382, 1.271)) * 1.7).sin() * 0.5 + 0.5) as f32;

    Some(AssetSiteSample {
        displaced_point: center.displaced_point,
        normal,
        biome0,
        biome1,
        slope_hint,
        curvature_hint,
        altitude_norm,
        mask_value,
    })
}

fn sample_surface_point(
    config: &RuntimeConfig,
    key: ChunkKey,
    chunk_uv: DVec2,
    terrain: &TerrainFieldSettings,
) -> Option<TerrainSample> {
    let face_uv = chunk_uv_to_face_uv(key, chunk_uv).ok()?;
    let cube_point = cube_point_for_face(key.face, face_uv_to_signed_coords(face_uv));
    let unit_dir = config
        .cube_projection
        .project(normalize_to_cube_surface(cube_point));
    let planet_point = unit_dir * terrain.planet_radius;
    let height = terrain
        .sample_height(unit_dir)
        .clamp(-terrain.height_amplitude, terrain.height_amplitude);
    let displaced_point = unit_dir * (terrain.planet_radius + height);

    Some(TerrainSample {
        chunk_uv,
        face_uv,
        face_st: face_uv_to_signed_coords(face_uv),
        cube_point,
        unit_dir,
        planet_point,
        height,
        displaced_point,
    })
}

fn respects_exclusion_radius(
    assets: &[AssetInstance],
    family_id: u16,
    origin: DVec3,
    exclusion_radius: f64,
) -> bool {
    assets
        .iter()
        .filter(|asset| asset.family_id == family_id)
        .all(|asset| asset.origin.distance(origin) >= exclusion_radius)
}

fn build_asset_instance(
    base_hash: u64,
    family: AssetFamilyDefinition,
    site: AssetSiteSample,
) -> AssetInstance {
    let normal = site.normal.normalize_or_zero();
    let reference = if normal.y.abs() < 0.95 {
        DVec3::Y
    } else {
        DVec3::X
    };
    let tangent = reference.cross(normal).normalize_or_zero();
    let bitangent = normal.cross(tangent).normalize_or_zero();
    let yaw = unit_f64(base_hash ^ 0x94D0_49BB_1331_11EB) * std::f64::consts::TAU;
    let sin_yaw = yaw.sin();
    let cos_yaw = yaw.cos();
    let basis_x = (tangent * cos_yaw + bitangent * sin_yaw).normalize_or_zero();
    let basis_z = normal.cross(basis_x).normalize_or_zero();
    let scale_jitter =
        (unit_f32(base_hash ^ 0xDB4F_0B91_75AE_2165) * 2.0 - 1.0) * family.scale_jitter;
    let scale = (family.base_scale + scale_jitter).max(0.1);

    AssetInstance {
        family_id: family.family_id,
        origin: site.displaced_point,
        basis_x,
        basis_y: normal,
        basis_z,
        scale,
        color_seed: splitmix64(base_hash ^ 0xD6E8_FD90_3A2B_1C6D) as u32,
    }
}

fn placement_hash(seed: u64, key: ChunkKey, cell_id: u32, family_id: u16) -> u64 {
    let mut value = seed;
    value ^= u64::from(face_ordinal(key.face)) << 61;
    value ^= u64::from(key.lod) << 56;
    value ^= u64::from(key.x) << 28;
    value ^= u64::from(key.y);
    value ^= u64::from(cell_id).rotate_left(17);
    value ^= u64::from(family_id).rotate_left(49);
    splitmix64(value)
}

fn face_ordinal(face: Face) -> u8 {
    match face {
        Face::Px => 0,
        Face::Nx => 1,
        Face::Py => 2,
        Face::Ny => 3,
        Face::Pz => 4,
        Face::Nz => 5,
    }
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

fn unit_f64(value: u64) -> f64 {
    ((value >> 11) as f64) * (1.0 / ((1u64 << 53) as f64))
}

fn unit_f32(value: u64) -> f32 {
    unit_f64(value) as f32
}
