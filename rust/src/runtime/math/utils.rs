use super::super::*;

pub(crate) fn dvec3_to_vector3(value: DVec3) -> Vector3 {
    Vector3::new(value.x as f32, value.y as f32, value.z as f32)
}

pub(crate) fn vector3_to_dvec3(value: Vector3) -> DVec3 {
    DVec3::new(f64::from(value.x), f64::from(value.y), f64::from(value.z))
}

pub(crate) fn dvec3_to_f32_array(value: DVec3) -> [f32; 3] {
    [value.x as f32, value.y as f32, value.z as f32]
}

pub(crate) fn planet_to_chunk_local_f32(
    point_planet: DVec3,
    chunk_origin_planet: DVec3,
) -> [f32; 3] {
    dvec3_to_f32_array(point_planet - chunk_origin_planet)
}

pub(crate) fn relative_transform(chunk_origin_planet: DVec3, origin_planet: DVec3) -> Transform3D {
    Transform3D::IDENTITY.translated(dvec3_to_vector3(chunk_origin_planet - origin_planet))
}

pub(crate) fn normalize_to_cube_surface(cube_point: DVec3) -> DVec3 {
    let max_axis = cube_point.abs().max_element();
    if max_axis <= f64::EPSILON {
        DVec3::ZERO
    } else {
        cube_point / max_axis
    }
}

pub(crate) fn clamp_grid_index(value: i32, samples_per_edge: u32) -> i32 {
    value.clamp(0, samples_per_edge.saturating_sub(1) as i32)
}

pub(crate) fn write_f32x3(bytes: &mut [u8], value: [f32; 3]) {
    bytes[0..4].copy_from_slice(&value[0].to_le_bytes());
    bytes[4..8].copy_from_slice(&value[1].to_le_bytes());
    bytes[8..12].copy_from_slice(&value[2].to_le_bytes());
}

pub(crate) fn write_f32x2(bytes: &mut [u8], value: [f32; 2]) {
    bytes[0..4].copy_from_slice(&value[0].to_le_bytes());
    bytes[4..8].copy_from_slice(&value[1].to_le_bytes());
}

pub(crate) fn write_rgba8(bytes: &mut [u8], value: [f32; 4]) {
    for (index, channel) in value.into_iter().enumerate() {
        bytes[index] = (channel.clamp(0.0, 1.0) * 255.0).round() as u8;
    }
}

pub(crate) fn copy_f32_slice_to_packed_bytes(bytes: &mut PackedByteArray, values: &[f32]) {
    let byte_len = std::mem::size_of_val(values);
    if bytes.len() != byte_len {
        bytes.resize(byte_len);
    }

    let dst = bytes.as_mut_slice();
    #[cfg(target_endian = "little")]
    {
        let src = unsafe { std::slice::from_raw_parts(values.as_ptr().cast::<u8>(), byte_len) };
        dst.copy_from_slice(src);
    }
    #[cfg(not(target_endian = "little"))]
    {
        for (chunk, value) in dst
            .chunks_exact_mut(std::mem::size_of::<f32>())
            .zip(values.iter())
        {
            chunk.copy_from_slice(&value.to_le_bytes());
        }
    }
}

pub(crate) fn copy_f32x4_slice_to_packed_bytes(bytes: &mut PackedByteArray, values: &[[f32; 4]]) {
    let byte_len = std::mem::size_of_val(values);
    if bytes.len() != byte_len {
        bytes.resize(byte_len);
    }

    let dst = bytes.as_mut_slice();
    #[cfg(target_endian = "little")]
    {
        let src = unsafe { std::slice::from_raw_parts(values.as_ptr().cast::<u8>(), byte_len) };
        dst.copy_from_slice(src);
    }
    #[cfg(not(target_endian = "little"))]
    {
        for (chunk, sample) in dst
            .chunks_exact_mut(std::mem::size_of::<[f32; 4]>())
            .zip(values.iter())
        {
            for (channel_bytes, value) in chunk
                .chunks_exact_mut(std::mem::size_of::<f32>())
                .zip(sample.iter())
            {
                channel_bytes.copy_from_slice(&value.to_le_bytes());
            }
        }
    }
}

pub(crate) fn distance_sort_key(distance: f64) -> u64 {
    if !distance.is_finite() {
        u64::MAX
    } else {
        (distance.max(0.0) * 1_000.0) as u64
    }
}

pub(crate) fn cpu_mesh_to_surface_arrays(mesh: &CpuMeshBuffers) -> Array<Variant> {
    let vertices = PackedVector3Array::from_iter(
        mesh.positions
            .iter()
            .copied()
            .map(|position| Vector3::new(position[0], position[1], position[2])),
    );
    let normals = PackedVector3Array::from_iter(
        mesh.normals
            .iter()
            .copied()
            .map(|normal| Vector3::new(normal[0], normal[1], normal[2])),
    );
    let colors = PackedColorArray::from_iter(
        mesh.colors
            .iter()
            .copied()
            .map(|color| Color::from_rgba(color[0], color[1], color[2], color[3])),
    );
    let uvs = PackedVector2Array::from_iter(
        mesh.uvs
            .iter()
            .copied()
            .map(|uv| Vector2::new(uv[0], uv[1])),
    );
    let indices = PackedInt32Array::from_iter(mesh.indices.iter().copied());

    Array::from_iter([
        vertices.to_variant(),
        normals.to_variant(),
        Variant::nil(),
        colors.to_variant(),
        uvs.to_variant(),
        Variant::nil(),
        Variant::nil(),
        Variant::nil(),
        Variant::nil(),
        Variant::nil(),
        Variant::nil(),
        Variant::nil(),
        indices.to_variant(),
    ])
}

pub(crate) fn collider_face_vertices_from_indices(
    collider_vertices: &[[f32; 3]],
    collider_indices: &[i32],
) -> Vec<[f32; 3]> {
    collider_indices
        .iter()
        .filter_map(|index| {
            let index = usize::try_from(*index).ok()?;
            collider_vertices.get(index).copied()
        })
        .collect()
}
