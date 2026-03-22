use super::*;

fn sample_surface_class() -> SurfaceClassKey {
    SurfaceClassKey::canonical_chunk(0b0101, 3, DEFAULT_RENDER_FORMAT_MASK, 12, 24, 4).unwrap()
}

fn test_runtime() -> PlanetRuntime {
    PlanetRuntime::new(
        RuntimeConfig {
            metadata_precompute_max_lod: 0,
            enable_godot_staging: false,
            ..RuntimeConfig::default()
        },
        Rid::Invalid,
        Rid::Invalid,
    )
}

fn sample_key() -> ChunkKey {
    ChunkKey::new(Face::Px, 2, 1, 1)
}

fn sample_meta(key: ChunkKey, surface_class: SurfaceClassKey) -> ChunkMeta {
    ChunkMeta::new(
        key,
        ChunkBounds::new(DVec3::new(1.0, 2.0, 3.0), 50.0, -20.0, 40.0, 80.0, 140.0),
        ChunkMetrics::new(2.5, 42.0, 0.25),
        surface_class,
    )
    .unwrap()
}

fn sample_payload(surface_class: &SurfaceClassKey, fill: u8) -> ChunkPayload {
    ChunkPayload {
        mesh: CpuMeshBuffers {
            positions: vec![[0.0, 0.0, 0.0]; surface_class.vertex_count as usize],
            indices: vec![0; surface_class.index_count as usize],
            ..CpuMeshBuffers::default()
        },
        packed_regions: Some(PackedMeshRegions {
            vertex_region: vec![fill; surface_class.vertex_bytes],
            attribute_region: vec![fill; surface_class.attribute_bytes],
            index_region: vec![fill; surface_class.index_bytes],
            vertex_stride: 12,
            attribute_stride: 24,
            index_stride: 4,
        }),
        ..ChunkPayload::default()
    }
}

fn huge_test_frustum() -> [Plane; 6] {
    box_test_frustum(Vector3::ZERO, 20_000.0)
}

fn box_test_frustum(center: Vector3, half_extent: f32) -> [Plane; 6] {
    [
        Plane::from_point_normal(
            center + Vector3::new(0.0, 0.0, half_extent),
            Vector3::new(0.0, 0.0, 1.0),
        ),
        Plane::from_point_normal(
            center + Vector3::new(0.0, 0.0, -half_extent),
            Vector3::new(0.0, 0.0, -1.0),
        ),
        Plane::from_point_normal(
            center + Vector3::new(-half_extent, 0.0, 0.0),
            Vector3::new(-1.0, 0.0, 0.0),
        ),
        Plane::from_point_normal(
            center + Vector3::new(0.0, half_extent, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
        ),
        Plane::from_point_normal(
            center + Vector3::new(half_extent, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
        ),
        Plane::from_point_normal(
            center + Vector3::new(0.0, -half_extent, 0.0),
            Vector3::new(0.0, -1.0, 0.0),
        ),
    ]
}

fn orbit_camera_state() -> CameraState {
    CameraState {
        position_planet: DVec3::new(0.0, 0.0, 3_000.0),
        forward_planet: DVec3::new(0.0, 0.0, -1.0),
        frustum_planes: huge_test_frustum(),
        projection_scale: 1_200.0,
        origin: OriginSnapshot::for_config(&RuntimeConfig::default(), DVec3::ZERO),
    }
}

fn near_surface_camera_state() -> CameraState {
    CameraState {
        position_planet: DVec3::new(0.0, 0.0, 1_000.0),
        forward_planet: DVec3::new(0.0, 0.0, -1.0),
        frustum_planes: huge_test_frustum(),
        projection_scale: 1_200.0,
        origin: OriginSnapshot::for_config(&RuntimeConfig::default(), DVec3::ZERO),
    }
}

fn local_position_to_dvec3(position: [f32; 3]) -> DVec3 {
    DVec3::new(
        f64::from(position[0]),
        f64::from(position[1]),
        f64::from(position[2]),
    )
}

fn visible_edge_vertex_index(edge: Edge, step: u32) -> usize {
    let last = mesh_topology::VISIBLE_VERTICES_PER_EDGE - 1;
    let (x, y) = match edge {
        Edge::NegU => (0, step),
        Edge::PosU => (last, step),
        Edge::NegV => (step, 0),
        Edge::PosV => (step, last),
    };
    (y * mesh_topology::VISIBLE_VERTICES_PER_EDGE + x) as usize
}

#[test]
fn chunk_key_validates_coords_against_lod_resolution() {
    assert!(ChunkKey::new(Face::Px, 3, 7, 7).is_valid_for_lod());
    assert!(!ChunkKey::new(Face::Px, 3, 8, 7).is_valid_for_lod());
    assert_eq!(ChunkKey::resolution_for_lod(5), 32);
}

#[test]
fn surface_class_mismatch_detection_is_strict() {
    let base = sample_surface_class();
    let mismatched =
        SurfaceClassKey::canonical_chunk(0b0101, 3, DEFAULT_RENDER_FORMAT_MASK, 16, 24, 4).unwrap();

    let issues = base.compatibility_issues(&mismatched);

    assert_eq!(issues, vec!["vertex_stride", "vertex_bytes"]);
    assert!(!base.is_pool_compatible_with(&mismatched));
    assert!(base.is_pool_compatible_with(&base));
}

#[test]
fn packed_region_validation_uses_precomputed_byte_expectations() {
    let surface_class = sample_surface_class();
    let valid = PackedMeshRegions {
        vertex_region: vec![0; surface_class.vertex_bytes],
        attribute_region: vec![0; surface_class.attribute_bytes],
        index_region: vec![0; surface_class.index_bytes],
        vertex_stride: 12,
        attribute_stride: 24,
        index_stride: 4,
    };
    let invalid = PackedMeshRegions {
        vertex_region: vec![0; surface_class.vertex_bytes.saturating_sub(1)],
        ..valid.clone()
    };

    assert_eq!(valid.validate_for_surface_class(&surface_class), Ok(()));
    assert_eq!(
        invalid.validate_for_surface_class(&surface_class),
        Err("vertex_bytes")
    );

    let wrong_stride = PackedMeshRegions {
        vertex_stride: 16,
        ..valid.clone()
    };
    assert_eq!(
        wrong_stride.validate_for_surface_class(&surface_class),
        Err("vertex_stride")
    );
}

#[test]
fn runtime_map_transitions_are_deterministic() {
    let key = sample_key();
    let surface_class = sample_surface_class();
    let meta = sample_meta(key, surface_class.clone());
    let payload = sample_payload(&surface_class, 1);

    let mut runtime = test_runtime();
    let initial_meta_count = runtime.meta_count();
    runtime.register_chunk_meta(meta).unwrap();
    runtime.activate_render(key);
    runtime.activate_physics(key);
    runtime.insert_payload(key, payload);
    runtime.ensure_rid_state(key).render_resident = true;
    runtime.ensure_rid_state(key).physics_resident = true;

    assert_eq!(runtime.meta_count(), initial_meta_count + 1);
    assert_eq!(runtime.active_render_count(), 1);
    assert_eq!(runtime.active_physics_count(), 1);
    assert_eq!(runtime.resident_payload_count(), 1);
    assert_eq!(runtime.rid_state_count(), 1);

    assert!(runtime.deactivate_render(&key));
    assert!(runtime.deactivate_physics(&key));
    assert!(runtime.remove_payload(&key).is_some());

    assert_eq!(runtime.active_render_count(), 0);
    assert_eq!(runtime.active_physics_count(), 0);
    assert_eq!(runtime.resident_payload_count(), 0);
    assert_eq!(runtime.rid_state_count(), 1);
}

#[test]
fn payload_precompute_window_stays_bounded() {
    let runtime = PlanetRuntime::default();

    assert!(runtime.should_precompute_payload_for_lod(PAYLOAD_PRECOMPUTE_MAX_LOD));
    assert!(!runtime.should_precompute_payload_for_lod(PAYLOAD_PRECOMPUTE_MAX_LOD + 1));
}

#[test]
fn metadata_precompute_window_is_explicit_in_runtime_config() {
    let runtime = PlanetRuntime::new(
        RuntimeConfig {
            metadata_precompute_max_lod: 2,
            enable_godot_staging: false,
            ..RuntimeConfig::default()
        },
        Rid::Invalid,
        Rid::Invalid,
    );

    assert_eq!(runtime.metadata_precompute_max_lod(), 2);
    assert_eq!(runtime.meta_count(), 6 * (1 + 4 + 16));
}

#[test]
fn threaded_payload_handoff_preserves_request_sequence() {
    let mut runtime = PlanetRuntime::new(
        RuntimeConfig {
            metadata_precompute_max_lod: 0,
            enable_godot_staging: false,
            worker_thread_count: 2,
            ..RuntimeConfig::default()
        },
        Rid::Invalid,
        Rid::Invalid,
    );
    let keys = vec![
        ChunkKey::new(Face::Pz, 2, 1, 0),
        ChunkKey::new(Face::Px, 2, 0, 1),
        ChunkKey::new(Face::Py, 2, 1, 1),
    ];
    let desired_render = keys.iter().copied().collect::<HashSet<_>>();
    let requests = keys
        .iter()
        .copied()
        .enumerate()
        .map(|(sequence, key)| {
            runtime
                .prepare_render_payload_request(sequence, key, &desired_render)
                .unwrap()
                .unwrap()
        })
        .collect::<Vec<_>>();

    let batch = runtime.threaded_payload_generator.generate(requests);
    let result_keys = batch
        .results
        .iter()
        .map(|payload| payload.key)
        .collect::<Vec<_>>();

    assert_eq!(result_keys, keys);
    assert_eq!(batch.results.len(), 3);
}

#[test]
fn phase7_pipeline_builds_sample_mesh_and_packed_payloads() {
    let mut runtime = test_runtime();
    let key = ChunkKey::new(Face::Pz, 2, 1, 1);
    let desired_render = [key].into_iter().collect::<HashSet<_>>();
    let mut frame_state = SelectionFrameState::default();

    let upload_bytes = runtime
        .ensure_render_payload_for_selection(key, &desired_render, &mut frame_state)
        .unwrap();
    let payload = runtime.resident_payloads.get(&key).unwrap();

    assert_eq!(
        payload.sample_count as u32,
        mesh_topology::SAMPLED_VERTICES_PER_EDGE * mesh_topology::SAMPLED_VERTICES_PER_EDGE
    );
    assert_eq!(
        payload.mesh.vertex_count() as u32,
        mesh_topology::canonical_chunk_topology().vertex_count()
    );
    assert_eq!(
        payload.mesh.index_count(),
        payload.surface_class.index_count as usize
    );
    assert_eq!(payload.stitch_mask, mesh_topology::BASE_STITCH_MASK);
    assert_eq!(payload.upload_bytes(), upload_bytes);
    assert!(payload.packed_regions.is_some());
    assert_eq!(frame_state.phase7_sampled_chunks, 1);
    assert_eq!(frame_state.phase7_meshed_chunks, 1);
    assert_eq!(frame_state.phase7_packed_chunks, 1);
    assert_eq!(frame_state.phase7_staged_chunks, 0);
    assert_eq!(frame_state.phase7_commit_payloads, 1);
}

#[test]
fn phase10_payload_vertices_are_chunk_local_offsets() {
    let mut runtime = test_runtime();
    let key = ChunkKey::new(Face::Pz, 2, 1, 1);
    let desired_render = [key].into_iter().collect::<HashSet<_>>();
    let mut frame_state = SelectionFrameState::default();

    runtime
        .ensure_render_payload_for_selection(key, &desired_render, &mut frame_state)
        .unwrap();
    let payload = runtime.resident_payloads.get(&key).unwrap();
    let samples = runtime.sample_chunk_scalar_field(key).unwrap();
    let visible_sample = samples.get(
        mesh_topology::BORDER_RING_QUADS,
        mesh_topology::BORDER_RING_QUADS,
    );

    let reconstructed =
        payload.chunk_origin_planet + local_position_to_dvec3(payload.mesh.positions[0]);
    let expected = visible_sample.displaced_point(runtime.config.planet_radius);

    assert!(
        (reconstructed - expected).length() < 1.0e-3,
        "reconstructed={reconstructed:?} expected={expected:?}"
    );
    assert!(payload.chunk_origin_planet.length() > 0.0);
}

#[test]
fn generated_mesh_triangles_use_godot_clockwise_front_faces() {
    let mut runtime = test_runtime();
    let key = ChunkKey::new(Face::Pz, 2, 1, 1);
    let chunk_origin = runtime.ensure_chunk_meta(key).unwrap().bounds.center_planet;
    let samples = runtime.sample_chunk_scalar_field(key).unwrap();
    let mesh = runtime
        .derive_cpu_mesh_buffers(&samples, mesh_topology::BASE_STITCH_MASK, chunk_origin)
        .unwrap();

    for triangle in mesh.indices.chunks_exact(3).take(64) {
        let a = chunk_origin + local_position_to_dvec3(mesh.positions[triangle[0] as usize]);
        let b = chunk_origin + local_position_to_dvec3(mesh.positions[triangle[1] as usize]);
        let c = chunk_origin + local_position_to_dvec3(mesh.positions[triangle[2] as usize]);
        let outward = ((b - a).cross(c - a)).normalize_or_zero();
        let triangle_center = (a + b + c) / 3.0;

        assert!(
            outward.dot(triangle_center.normalize_or_zero()) < 0.0,
            "triangle winding was not clockwise/front-facing for Godot: {triangle:?}"
        );
    }
}

#[test]
fn generated_mesh_normals_point_outward() {
    let mut runtime = test_runtime();
    let key = ChunkKey::new(Face::Pz, 2, 1, 1);
    let chunk_origin = runtime.ensure_chunk_meta(key).unwrap().bounds.center_planet;
    let samples = runtime.sample_chunk_scalar_field(key).unwrap();
    let mesh = runtime
        .derive_cpu_mesh_buffers(&samples, mesh_topology::BASE_STITCH_MASK, chunk_origin)
        .unwrap();

    for (position, normal) in mesh.positions.iter().zip(mesh.normals.iter()).take(64) {
        let absolute_position = chunk_origin + local_position_to_dvec3(*position);
        let outward = absolute_position.normalize_or_zero();
        let normal = local_position_to_dvec3(*normal).normalize_or_zero();

        assert!(
            normal.dot(outward) > 0.0,
            "normal was not outward-facing: position={absolute_position:?} normal={normal:?}"
        );
    }
}

#[test]
fn rendered_chunk_edges_match_across_cross_face_seams() {
    let mut runtime = test_runtime();
    let key = ChunkKey::new(Face::Px, 0, 0, 0);
    let edge = Edge::PosU;
    let neighbor = topology::same_lod_neighbor(key, edge).unwrap();
    let xform = topology::edge_transform(key.face, edge);
    let last = mesh_topology::VISIBLE_VERTICES_PER_EDGE - 1;

    let chunk_origin = runtime.ensure_chunk_meta(key).unwrap().bounds.center_planet;
    let chunk_samples = runtime.sample_chunk_scalar_field(key).unwrap();
    let chunk_mesh = runtime
        .derive_cpu_mesh_buffers(
            &chunk_samples,
            mesh_topology::BASE_STITCH_MASK,
            chunk_origin,
        )
        .unwrap();

    let neighbor_origin = runtime
        .ensure_chunk_meta(neighbor)
        .unwrap()
        .bounds
        .center_planet;
    let neighbor_samples = runtime.sample_chunk_scalar_field(neighbor).unwrap();
    let neighbor_mesh = runtime
        .derive_cpu_mesh_buffers(
            &neighbor_samples,
            mesh_topology::BASE_STITCH_MASK,
            neighbor_origin,
        )
        .unwrap();

    for step in 0..=last {
        let chunk_index = visible_edge_vertex_index(edge, step);
        let mapped_step = if xform.flip { last - step } else { step };
        let neighbor_index = visible_edge_vertex_index(xform.neighbor_edge, mapped_step);

        let chunk_position =
            chunk_origin + local_position_to_dvec3(chunk_mesh.positions[chunk_index]);
        let neighbor_position =
            neighbor_origin + local_position_to_dvec3(neighbor_mesh.positions[neighbor_index]);

        assert!(
            (chunk_position - neighbor_position).length() < 1.0e-3,
            "seam mismatch at step {step}: {chunk_position:?} vs {neighbor_position:?}"
        );
    }
}

#[test]
fn phase10_origin_policy_keeps_render_and_physics_origins_in_lockstep() {
    let mut runtime = test_runtime();
    let camera_planet = DVec3::new(12_345.0, -678.0, 9_101.0);

    assert!(runtime.update_origin_from_camera(camera_planet));
    assert_eq!(
        runtime.origin_snapshot.mode,
        OriginPolicyMode::SharedCameraRelative
    );
    assert_eq!(runtime.origin_snapshot.render_origin_planet, camera_planet);
    assert_eq!(runtime.origin_snapshot.physics_origin_planet, camera_planet);
}

#[test]
fn phase10_frustum_checks_use_render_relative_centers() {
    let runtime = test_runtime();
    let key = sample_key();
    let surface_class = sample_surface_class();
    let meta = ChunkMeta::new(
        key,
        ChunkBounds::new(
            DVec3::new(50_000.0, 0.0, 0.0),
            200.0,
            -10.0,
            10.0,
            990.0,
            1_010.0,
        ),
        ChunkMetrics::new(1.0, 0.0, 0.05),
        surface_class,
    )
    .unwrap();

    let unshifted = CameraState {
        position_planet: DVec3::new(50_000.0, 0.0, 500.0),
        forward_planet: DVec3::new(0.0, 0.0, -1.0),
        frustum_planes: box_test_frustum(Vector3::ZERO, 1_000.0),
        projection_scale: 1_200.0,
        origin: OriginSnapshot::for_config(&RuntimeConfig::default(), DVec3::ZERO),
    };
    let shifted = CameraState {
        position_planet: DVec3::new(50_000.0, 0.0, 500.0),
        forward_planet: DVec3::new(0.0, 0.0, -1.0),
        frustum_planes: box_test_frustum(Vector3::ZERO, 1_000.0),
        projection_scale: 1_200.0,
        origin: OriginSnapshot::for_config(
            &RuntimeConfig::default(),
            DVec3::new(50_000.0, 0.0, 0.0),
        ),
    };

    assert!(!runtime.frustum_visible(&unshifted, &meta));
    assert!(runtime.frustum_visible(&shifted, &meta));
}

#[test]
fn threaded_payload_generation_reuses_worker_scratch_on_follow_up_batch() {
    let mut runtime = PlanetRuntime::new(
        RuntimeConfig {
            metadata_precompute_max_lod: 0,
            enable_godot_staging: false,
            worker_thread_count: 1,
            ..RuntimeConfig::default()
        },
        Rid::Invalid,
        Rid::Invalid,
    );
    let keys = [
        ChunkKey::new(Face::Pz, 2, 1, 1),
        ChunkKey::new(Face::Px, 2, 0, 1),
    ];
    let desired_render = keys.iter().copied().collect::<HashSet<_>>();

    let mut first_frame = SelectionFrameState::default();
    runtime
        .prepare_render_payloads_for_selection(&keys, &desired_render, &mut first_frame)
        .unwrap();
    for key in keys.iter().copied() {
        runtime.remove_payload(&key);
    }

    let mut second_frame = SelectionFrameState::default();
    runtime
        .prepare_render_payloads_for_selection(&keys, &desired_render, &mut second_frame)
        .unwrap();

    assert_eq!(second_frame.phase9_worker_threads, 1);
    assert_eq!(second_frame.phase9_generation_jobs, keys.len());
    assert!(
        second_frame.phase9_sample_scratch_reuse_hits > 0
            || second_frame.phase9_mesh_scratch_reuse_hits > 0
            || second_frame.phase9_pack_scratch_reuse_hits > 0
    );
}

#[test]
fn selection_driven_surface_class_marks_fine_to_coarse_stitch_edges() {
    let mut runtime = test_runtime();
    let fine_key = ChunkKey::new(Face::Px, 2, 1, 1);
    let coarse_neighbor_cover = ChunkKey::new(Face::Px, 1, 0, 0);
    let desired_render = [fine_key, coarse_neighbor_cover]
        .into_iter()
        .collect::<HashSet<_>>();

    let surface_class = runtime
        .required_surface_class_for_selection(fine_key, &desired_render)
        .unwrap();

    assert_eq!(
        surface_class.stitch_mask,
        mesh_topology::stitch_mask_bit(Edge::NegU) | mesh_topology::stitch_mask_bit(Edge::NegV)
    );
}

#[test]
fn payload_residency_budget_stays_bounded_under_mock_camera_churn() {
    let surface_class = sample_surface_class();
    let mut runtime = test_runtime();

    for x in 0..8 {
        let key = ChunkKey::new(Face::Px, 3, x, 0);
        runtime.insert_payload(key, sample_payload(&surface_class, x as u8));
    }

    for center_x in 1..=6 {
        let retained_keys = [
            ChunkKey::new(Face::Px, 3, center_x - 1, 0),
            ChunkKey::new(Face::Px, 3, center_x, 0),
            ChunkKey::new(Face::Px, 3, center_x + 1, 0),
        ]
        .into_iter()
        .collect::<HashSet<_>>();

        for key in retained_keys.iter().copied() {
            runtime
                .resident_payloads
                .entry(key)
                .or_insert_with(|| sample_payload(&surface_class, key.x as u8));
        }

        let _ = runtime.enforce_payload_residency_budget(&retained_keys, 4);

        assert!(runtime.resident_payload_count() <= 4);
        assert!(retained_keys
            .iter()
            .all(|key| runtime.resident_payloads.contains_key(key)));
    }
}

#[test]
fn incompatible_warm_reuse_routes_to_compatible_pool_or_cold_path() {
    let current_surface_class = sample_surface_class();
    let required_surface_class =
        SurfaceClassKey::canonical_chunk(0b0011, 3, DEFAULT_RENDER_FORMAT_MASK, 12, 24, 4).unwrap();
    let pooled_entry = RenderPoolEntry {
        mesh_rid: Rid::Invalid,
        render_instance_rid: Rid::Invalid,
        gd_staging: None,
        surface_class: required_surface_class.clone(),
    };

    let mut runtime = test_runtime();
    runtime.push_render_pool_entry(pooled_entry.clone());

    let reused =
        runtime.choose_render_warm_path(Some(&current_surface_class), &required_surface_class);
    assert_eq!(reused, RenderWarmPath::ReusePooledSurface(pooled_entry));

    let mut runtime = test_runtime();
    let cold_path =
        runtime.choose_render_warm_path(Some(&current_surface_class), &required_surface_class);
    assert_eq!(
        cold_path,
        RenderWarmPath::ColdPath(RenderFallbackReason::IncompatibleCurrentSurfaceClass(vec![
            "stitch_mask",
            "index_class",
            "index_count",
            "index_bytes"
        ]))
    );
}

#[test]
fn register_chunk_meta_recomputes_neighbors_from_phase4_topology() {
    let key = ChunkKey::new(Face::Px, 2, 0, 0);
    let surface_class = sample_surface_class();
    let mut meta = ChunkMeta::new(
        key,
        ChunkBounds::new(DVec3::new(0.0, 0.0, 0.0), 10.0, -1.0, 1.0, 999.0, 1001.0),
        ChunkMetrics::new(1.0, 5.0, 0.1),
        surface_class,
    )
    .unwrap();

    meta.neighbors = ChunkNeighbors::new([
        ChunkKey::new(Face::Nx, 2, 0, 0),
        ChunkKey::new(Face::Nx, 2, 0, 0),
        ChunkKey::new(Face::Nx, 2, 0, 0),
        ChunkKey::new(Face::Nx, 2, 0, 0),
    ]);

    let mut runtime = test_runtime();
    runtime.register_chunk_meta(meta).unwrap();

    let stored = runtime.meta.get(&key).unwrap();
    assert_eq!(
        stored.neighbors.get(Edge::NegU),
        ChunkKey::new(Face::Pz, 2, 3, 0)
    );
    assert_eq!(
        stored.neighbors.get(Edge::PosU),
        ChunkKey::new(Face::Px, 2, 1, 0)
    );
    assert_eq!(
        stored.neighbors.get(Edge::NegV),
        ChunkKey::new(Face::Ny, 2, 3, 3)
    );
    assert_eq!(
        stored.neighbors.get(Edge::PosV),
        ChunkKey::new(Face::Px, 2, 0, 1)
    );
}

#[test]
fn ensure_chunk_meta_lazily_builds_phase6_bounds_and_surface_class() {
    let mut runtime = test_runtime();
    let key = ChunkKey::new(Face::Pz, 3, 5, 2);
    let initial_meta_count = runtime.meta_count();

    let meta = runtime.ensure_chunk_meta(key).unwrap().clone();

    assert_eq!(meta.key, key);
    assert_eq!(runtime.meta_count(), initial_meta_count + 1);
    assert!(meta.bounds.radius > 0.0);
    assert!(meta.metrics.angular_radius > 0.0);
    assert!(meta.metrics.geometric_error > 0.0);
    assert_eq!(
        meta.surface_class.stitch_mask,
        mesh_topology::BASE_STITCH_MASK
    );
    assert_eq!(
        meta.surface_class.vertex_stride,
        DEFAULT_RENDER_VERTEX_STRIDE
    );
}

#[test]
fn chunk_meta_tracks_chunk_local_height_extrema() {
    let mut runtime = test_runtime();
    let a = runtime
        .ensure_chunk_meta(ChunkKey::new(Face::Px, 3, 0, 0))
        .unwrap()
        .clone();
    let b = runtime
        .ensure_chunk_meta(ChunkKey::new(Face::Px, 3, 7, 7))
        .unwrap()
        .clone();

    assert!(a.bounds.min_height >= -(runtime.config.height_amplitude as f32));
    assert!(a.bounds.max_height <= runtime.config.height_amplitude as f32);
    assert!(
        a.bounds.min_height != b.bounds.min_height || a.bounds.max_height != b.bounds.max_height
    );
}

#[test]
fn horizon_culling_keeps_emergent_chunks_without_global_disable() {
    let runtime = test_runtime();
    let camera = near_surface_camera_state();
    let surface_class = sample_surface_class();
    let emergent_meta = ChunkMeta::new(
        sample_key(),
        ChunkBounds::new(
            DVec3::new(0.0, 0.574, 0.819),
            40.0,
            -10.0,
            120.0,
            990.0,
            1_120.0,
        ),
        ChunkMetrics::new(1.0, 0.0, 0.03),
        surface_class.clone(),
    )
    .unwrap();
    let buried_meta = ChunkMeta::new(
        sample_key(),
        ChunkBounds::new(
            DVec3::new(0.0, 0.574, 0.819),
            40.0,
            -120.0,
            -120.0,
            880.0,
            880.0,
        ),
        ChunkMetrics::new(1.0, 0.0, 0.03),
        surface_class,
    )
    .unwrap();

    assert!(runtime.horizon_visible(&camera, &emergent_meta));
    assert!(!runtime.horizon_visible(&camera, &buried_meta));
}

#[test]
fn selector_normalizes_neighbor_lod_delta_to_one() {
    let mut runtime = test_runtime();
    let camera = orbit_camera_state();

    runtime.step_visibility_selection(&camera).unwrap();

    for key in runtime.active_render.iter().copied() {
        for edge in Edge::ALL {
            let neighbor_same_lod = topology::same_lod_neighbor(key, edge).unwrap();
            if let Some(neighbor) = PlanetRuntime::find_active_ancestor_covering(
                neighbor_same_lod,
                &runtime.active_render,
            ) {
                assert!(
                    key.lod.abs_diff(neighbor.lod) <= 1,
                    "neighbor lod delta exceeded 1: {key:?} vs {neighbor:?}"
                );
            }
        }
    }
}

#[test]
fn budgeted_selector_defers_work_when_frame_budget_is_tight() {
    let mut runtime = PlanetRuntime::new(
        RuntimeConfig {
            metadata_precompute_max_lod: 0,
            enable_godot_staging: false,
            commit_budget_per_frame: 2,
            upload_budget_bytes_per_frame: 128,
            ..RuntimeConfig::default()
        },
        Rid::Invalid,
        Rid::Invalid,
    );
    let camera = orbit_camera_state();

    runtime.step_visibility_selection(&camera).unwrap();

    assert!(runtime.desired_render_count() > runtime.active_render_count());
    assert!(runtime.deferred_commit_count() > 0);
    assert!(runtime.deferred_upload_bytes() > 0);
    assert!(runtime.frame_state.max_deferred_starvation_frames >= 1);
}

#[test]
fn per_kind_commit_budgets_cap_render_and_physics_spikes() {
    let mut runtime = PlanetRuntime::new(
        RuntimeConfig {
            metadata_precompute_max_lod: 0,
            enable_godot_staging: false,
            commit_budget_per_frame: 16,
            upload_budget_bytes_per_frame: usize::MAX,
            render_activation_budget_per_frame: 1,
            render_update_budget_per_frame: 0,
            physics_activation_budget_per_frame: 0,
            ..RuntimeConfig::default()
        },
        Rid::Invalid,
        Rid::Invalid,
    );
    let camera = orbit_camera_state();

    runtime.step_visibility_selection(&camera).unwrap();

    assert_eq!(runtime.active_render_count(), 1);
    assert_eq!(runtime.active_physics_count(), 0);
    assert!(runtime.deferred_commit_count() > 0);
}

#[test]
fn physics_active_set_stays_separate_from_render_set() {
    let mut runtime = PlanetRuntime::new(
        RuntimeConfig {
            metadata_precompute_max_lod: 0,
            enable_godot_staging: false,
            physics_activation_radius: 10_000.0,
            physics_max_active_chunks: 4,
            ..RuntimeConfig::default()
        },
        Rid::Invalid,
        Rid::Invalid,
    );
    let camera = orbit_camera_state();

    for _ in 0..8 {
        runtime.step_visibility_selection(&camera).unwrap();
        if runtime.active_physics_count() > 0 {
            break;
        }
    }

    assert!(runtime.active_render_count() > 0);
    assert!(runtime.active_physics_count() > 0);
    assert!(runtime.active_physics_count() < runtime.active_render_count());
    assert!(runtime
        .active_physics
        .iter()
        .all(|key| runtime.active_render.contains(key)));
}

#[test]
fn physics_selection_respects_active_chunk_cap() {
    let mut runtime = PlanetRuntime::new(
        RuntimeConfig {
            metadata_precompute_max_lod: 0,
            enable_godot_staging: false,
            physics_activation_radius: 10_000.0,
            physics_max_active_chunks: 3,
            ..RuntimeConfig::default()
        },
        Rid::Invalid,
        Rid::Invalid,
    );
    let camera = orbit_camera_state();
    let desired_render = [
        ChunkKey::new(Face::Px, 2, 0, 0),
        ChunkKey::new(Face::Px, 2, 1, 0),
        ChunkKey::new(Face::Px, 2, 0, 1),
        ChunkKey::new(Face::Px, 2, 1, 1),
        ChunkKey::new(Face::Pz, 2, 1, 1),
    ]
    .into_iter()
    .collect::<HashSet<_>>();

    let physics = runtime
        .select_physics_set(&camera, &desired_render)
        .unwrap();

    assert_eq!(physics.len(), 3);
    assert!(physics.iter().all(|key| desired_render.contains(key)));
}

#[test]
fn phase8_warm_pooled_commit_recycles_previous_render_entry() {
    let mut runtime = PlanetRuntime::new(
        RuntimeConfig {
            metadata_precompute_max_lod: 0,
            enable_godot_staging: false,
            render_pool_watermark_per_class: 4,
            ..RuntimeConfig::default()
        },
        Rid::Invalid,
        Rid::Invalid,
    );
    let key = sample_key();
    let previous_surface_class = sample_surface_class();
    let next_surface_class =
        SurfaceClassKey::canonical_chunk(0b0011, 3, DEFAULT_RENDER_FORMAT_MASK, 12, 24, 4).unwrap();
    let previous_mesh_rid = Rid::Invalid;
    let previous_instance_rid = Rid::new(101);
    let pooled_mesh_rid = Rid::new(202);
    let pooled_instance_rid = Rid::new(303);

    runtime.install_render_entry(
        key,
        previous_mesh_rid,
        previous_instance_rid,
        previous_surface_class.clone(),
        None,
    );
    runtime.resident_payloads.insert(
        key,
        ChunkPayload {
            surface_class: next_surface_class.clone(),
            pooled_render_entry: Some(RenderPoolEntry {
                mesh_rid: pooled_mesh_rid,
                render_instance_rid: pooled_instance_rid,
                surface_class: next_surface_class.clone(),
                gd_staging: None,
            }),
            render_lifecycle: RenderLifecycleCommand::WarmReusePooled,
            ..sample_payload(&next_surface_class, 7)
        },
    );

    let mut frame_state = SelectionFrameState::default();
    runtime.commit_render_payload(key, &mut frame_state);

    let rid_state = runtime.rid_state.get(&key).unwrap();
    assert_eq!(rid_state.mesh_rid, Some(pooled_mesh_rid));
    assert_eq!(rid_state.render_instance_rid, Some(pooled_instance_rid));
    assert_eq!(
        rid_state.active_surface_class.as_ref(),
        Some(&next_surface_class)
    );
    assert_eq!(frame_state.phase8_render_warm_pool_commits, 1);

    let pooled_previous = runtime
        .pop_render_pool_entry(&previous_surface_class)
        .unwrap();
    assert_eq!(pooled_previous.mesh_rid, previous_mesh_rid);
    assert_eq!(pooled_previous.render_instance_rid, previous_instance_rid);
}

#[test]
fn phase8_pool_watermarks_bound_recycled_entries() {
    let surface_class = sample_surface_class();
    let mut runtime = PlanetRuntime::new(
        RuntimeConfig {
            metadata_precompute_max_lod: 0,
            enable_godot_staging: false,
            render_pool_watermark_per_class: 1,
            physics_pool_watermark: 1,
            ..RuntimeConfig::default()
        },
        Rid::Invalid,
        Rid::Invalid,
    );

    runtime.recycle_render_entry(RenderPoolEntry {
        mesh_rid: Rid::new(1),
        render_instance_rid: Rid::new(2),
        surface_class: surface_class.clone(),
        gd_staging: None,
    });
    runtime.recycle_render_entry(RenderPoolEntry {
        mesh_rid: Rid::new(3),
        render_instance_rid: Rid::new(4),
        surface_class: surface_class.clone(),
        gd_staging: None,
    });
    runtime.recycle_physics_entry(PhysicsPoolEntry {
        physics_body_rid: Rid::new(5),
        physics_shape_rid: Rid::new(6),
    });
    runtime.recycle_physics_entry(PhysicsPoolEntry {
        physics_body_rid: Rid::new(7),
        physics_shape_rid: Rid::new(8),
    });

    assert_eq!(runtime.render_pool_entry_count(), 1);
    assert_eq!(runtime.physics_pool.len(), 1);
}
