use super::*;
use std::time::Duration;

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

fn step_runtime_until_streaming_settles(
    runtime: &mut PlanetRuntime,
    camera: &CameraState,
    max_steps: usize,
) {
    let mut settled_streak = 0usize;
    for _ in 0..max_steps {
        runtime.step_visibility_selection(camera).unwrap();
        let settled = runtime.pending_payload_requests.is_empty()
            && runtime.pending_meta_requests.is_empty()
            && runtime.pending_asset_group_epoch.is_none()
            && !runtime.asset_groups_dirty
            && runtime.deferred_commit_count() == 0
            && runtime.active_render_count() == runtime.desired_render_count()
            && runtime.active_physics_count() == runtime.desired_physics_count();
        if settled {
            settled_streak += 1;
            if settled_streak >= 2 {
                break;
            }
        } else {
            settled_streak = 0;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
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

fn opposite_edge(edge: Edge) -> Edge {
    match edge {
        Edge::NegU => Edge::PosU,
        Edge::PosU => Edge::NegU,
        Edge::NegV => Edge::PosV,
        Edge::PosV => Edge::NegV,
    }
}

fn boundary_key(face: Face, lod: u8, edge: Edge, seam_index: u32) -> ChunkKey {
    let resolution = ChunkKey::resolution_for_lod(lod);
    match edge {
        Edge::NegU => ChunkKey::new(face, lod, 0, seam_index),
        Edge::PosU => ChunkKey::new(face, lod, resolution - 1, seam_index),
        Edge::NegV => ChunkKey::new(face, lod, seam_index, 0),
        Edge::PosV => ChunkKey::new(face, lod, seam_index, resolution - 1),
    }
}

fn stitched_edge_uses_vertex(mesh: &CpuMeshBuffers, vertex_index: usize) -> bool {
    mesh.indices
        .iter()
        .copied()
        .any(|index| usize::try_from(index).ok() == Some(vertex_index))
}

fn coarse_step_for_cover(fine_key: ChunkKey, edge: Edge, fine_step: u32) -> u32 {
    debug_assert_eq!(fine_step % 2, 0);
    let half_edge = mesh_topology::QUADS_PER_EDGE / 2;

    match edge {
        Edge::NegU | Edge::PosU => (fine_key.y % 2) * half_edge + fine_step / 2,
        Edge::NegV | Edge::PosV => (fine_key.x % 2) * half_edge + fine_step / 2,
    }
}

fn asset_group_summary(runtime: &PlanetRuntime) -> Vec<(AssetGroupKey, usize, Vec<ChunkKey>)> {
    let mut summary = runtime
        .asset_groups
        .values()
        .map(|state| (state.key, state.instance_count, state.source_chunks.clone()))
        .collect::<Vec<_>>();
    summary.sort_by(|a, b| a.0.cmp(&b.0));
    summary
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

    assert_eq!(runtime.meta_count(), initial_meta_count);
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
fn radius_derived_max_lod_keeps_average_chunk_span_above_target() {
    let derived = radius_derived_max_lod_for_planet_radius(
        1_000.0,
        DEFAULT_MIN_AVERAGE_CHUNK_SURFACE_SPAN_METERS,
        crate::topology::DEFAULT_MAX_LOD,
    );

    assert_eq!(derived, 5);
    assert!(
        average_chunk_surface_span_meters(1_000.0, derived)
            >= DEFAULT_MIN_AVERAGE_CHUNK_SURFACE_SPAN_METERS
    );
    assert!(
        average_chunk_surface_span_meters(1_000.0, derived + 1)
            < DEFAULT_MIN_AVERAGE_CHUNK_SURFACE_SPAN_METERS
    );
}

#[test]
fn runtime_config_normalization_recomputes_radius_derived_lod_from_planet_radius() {
    let runtime = PlanetRuntime::new(
        RuntimeConfig {
            planet_radius: 4_000.0,
            max_lod_cap: crate::topology::DEFAULT_MAX_LOD,
            metadata_precompute_max_lod: 9,
            payload_precompute_max_lod: 9,
            ..RuntimeConfig::default()
        },
        Rid::Invalid,
        Rid::Invalid,
    );

    assert_eq!(
        runtime.config.max_lod_policy,
        MaxLodPolicyKind::RadiusDerived
    );
    assert_eq!(runtime.config.max_lod, 7);
    assert_eq!(runtime.config.metadata_precompute_max_lod, 7);
    assert_eq!(runtime.config.payload_precompute_max_lod, 7);
}

#[test]
fn fixed_max_lod_policy_still_allows_manual_override() {
    let runtime = PlanetRuntime::new(
        RuntimeConfig {
            planet_radius: 1_000.0,
            max_lod_policy: MaxLodPolicyKind::Fixed,
            max_lod: 3,
            max_lod_cap: crate::topology::DEFAULT_MAX_LOD,
            metadata_precompute_max_lod: 5,
            payload_precompute_max_lod: 5,
            ..RuntimeConfig::default()
        },
        Rid::Invalid,
        Rid::Invalid,
    );

    assert_eq!(runtime.config.max_lod_policy, MaxLodPolicyKind::Fixed);
    assert_eq!(runtime.config.max_lod, 3);
    assert_eq!(runtime.config.metadata_precompute_max_lod, 3);
    assert_eq!(runtime.config.payload_precompute_max_lod, 3);
}

#[test]
fn phase13_default_runtime_config_matches_documented_starting_values() {
    let config = RuntimeConfig::default();
    let runtime = PlanetRuntime::new(config.clone(), Rid::Invalid, Rid::Invalid);

    assert_eq!(config.max_lod_policy, MaxLodPolicyKind::RadiusDerived);
    assert_eq!(config.max_lod_cap, crate::topology::DEFAULT_MAX_LOD);
    assert_eq!(config.max_lod, 5);
    assert_eq!(config.payload_precompute_max_lod, 5);
    assert_eq!(config.metadata_precompute_max_lod, 5);
    assert!(config.enable_frustum_culling);
    assert!(!config.keep_coarse_lod_chunks_rendered);
    assert_eq!(config.split_threshold_px, DEFAULT_SPLIT_THRESHOLD_PX);
    assert_eq!(config.merge_threshold_px, DEFAULT_MERGE_THRESHOLD_PX);
    assert_eq!(config.horizon_safety_margin, DEFAULT_HORIZON_SAFETY_MARGIN);
    assert_eq!(
        config.physics_activation_radius,
        DEFAULT_PHYSICS_ACTIVATION_RADIUS
    );
    assert_eq!(
        config.physics_max_active_chunks,
        DEFAULT_PHYSICS_MAX_ACTIVE_CHUNKS
    );
    assert_eq!(
        config.commit_budget_per_frame,
        DEFAULT_COMMIT_BUDGET_PER_FRAME
    );
    assert_eq!(
        config.upload_budget_bytes_per_frame,
        DEFAULT_UPLOAD_BUDGET_BYTES_PER_FRAME
    );
    assert_eq!(
        config.render_activation_budget_per_frame,
        DEFAULT_RENDER_ACTIVATION_BUDGET_PER_FRAME
    );
    assert_eq!(
        config.render_update_budget_per_frame,
        DEFAULT_RENDER_UPDATE_BUDGET_PER_FRAME
    );
    assert_eq!(
        config.render_deactivation_budget_per_frame,
        DEFAULT_RENDER_DEACTIVATION_BUDGET_PER_FRAME
    );
    assert_eq!(
        config.physics_activation_budget_per_frame,
        DEFAULT_PHYSICS_ACTIVATION_BUDGET_PER_FRAME
    );
    assert_eq!(
        config.physics_deactivation_budget_per_frame,
        DEFAULT_PHYSICS_DEACTIVATION_BUDGET_PER_FRAME
    );
    assert_eq!(
        config.render_pool_watermark_per_class,
        DEFAULT_RENDER_POOL_WATERMARK_PER_CLASS
    );
    assert_eq!(
        config.physics_pool_watermark,
        DEFAULT_PHYSICS_POOL_WATERMARK
    );
    assert_eq!(
        average_chunk_surface_span_meters(config.planet_radius, config.max_lod),
        average_chunk_surface_span_meters(runtime.config.planet_radius, runtime.config.max_lod)
    );
    assert_eq!(runtime.config.max_lod, config.max_lod);
    assert!(config.physics_pool_watermark < config.render_pool_watermark_per_class);
    assert!((1..=DEFAULT_MAX_WORKER_THREADS).contains(&config.worker_thread_count));
    assert_eq!(runtime.worker_thread_count(), config.worker_thread_count);
}

#[test]
fn radius_derived_max_lod_honors_configured_cap() {
    let derived = radius_derived_max_lod_for_planet_radius(
        100_000.0,
        DEFAULT_MIN_AVERAGE_CHUNK_SURFACE_SPAN_METERS,
        14,
    );
    assert_eq!(derived, 12);

    let capped = RuntimeConfig {
        planet_radius: 100_000.0,
        max_lod_cap: 10,
        metadata_precompute_max_lod: 14,
        payload_precompute_max_lod: 14,
        ..RuntimeConfig::default()
    }
    .normalized();
    assert_eq!(capped.max_lod, 10);
    assert_eq!(capped.metadata_precompute_max_lod, 10);
    assert_eq!(capped.payload_precompute_max_lod, 10);
}

#[test]
fn phase14_build_order_sequence_is_contiguous_and_complete() {
    let stages = PlanetRuntime::build_order_stages();

    assert!(PlanetRuntime::build_order_is_contiguous());
    assert_eq!(PlanetRuntime::build_order_stage_count(), 23);
    assert_eq!(stages.first().map(|stage| stage.step), Some(1));
    assert_eq!(
        stages.first().map(|stage| stage.slug),
        Some("face-basis-neighbors")
    );
    assert_eq!(stages.last().map(|stage| stage.step), Some(23));
    assert_eq!(
        stages.last().map(|stage| stage.slug),
        Some("budgeting-polish")
    );
    assert!(stages.iter().all(|stage| !stage.description.is_empty()));
}

#[test]
fn build_order_handoff_summary_matches_documented_phase_continuity() {
    let runtime = test_runtime();
    let handoffs = PlanetRuntime::build_order_handoffs();
    let summary = runtime.build_order_summary();

    assert_eq!(handoffs[0].covered_step_range, Some((1, 20)));
    assert_eq!(handoffs[1].emphasized_steps, [5, 8, 19]);
    assert_eq!(handoffs[2].covered_step_range, Some((21, 21)));
    assert_eq!(handoffs[3].covered_step_range, Some((22, 22)));
    assert_eq!(handoffs[4].covered_step_range, Some((23, 23)));
    assert!(summary.contains("phase=15"));
    assert!(summary.contains("phases01-10=1-20"));
    assert!(summary.contains("phase11=doc+5/8/19"));
    assert!(summary.contains("phase12=21"));
    assert!(summary.contains("phase09=22"));
    assert!(summary.contains("phase13=23"));
    assert!(summary.contains(NEXT_PHASE_LABEL));
}

#[test]
fn phase15_default_strategy_summary_matches_documented_stack() {
    let runtime = test_runtime();
    let config = &runtime.config;
    let summary = runtime.strategy_summary();

    assert_eq!(
        ProjectionStrategy::label(&config.cube_projection),
        "spherified_cube"
    );
    assert_eq!(
        ChunkVisibilityStrategy::label(&config.visibility_strategy),
        "horizon_frustum_lod"
    );
    assert_eq!(
        ChunkRenderBackend::label(&config.render_backend),
        "server_pool_render_backend"
    );
    assert_eq!(
        PackedStagingPolicy::label(&config.staging_policy),
        "godot_owned_packed_byte_array"
    );
    assert!(summary.contains("projection=spherified_cube"));
    assert!(summary.contains("visibility=horizon_frustum_lod"));
    assert!(summary.contains("render_backend=server_pool_render_backend"));
    assert!(summary.contains("staging=godot_owned_packed_byte_array"));
}

#[test]
fn phase15_projection_strategy_remains_swappable() {
    let cube_point = DVec3::new(1.0, 0.31, -0.47);
    let normalized = ProjectionStrategy::project(&CubeProjection::Normalized, cube_point);
    let spherified = ProjectionStrategy::project(&CubeProjection::Spherified, cube_point);

    assert!((normalized.length() - 1.0).abs() <= 1.0e-12);
    assert!((spherified.length() - 1.0).abs() <= 1.0e-12);
    assert!(normalized.distance(spherified) > 1.0e-4);
}

#[test]
fn phase15_visibility_strategy_matches_runtime_wrappers() {
    let mut runtime = test_runtime();
    let key = ChunkKey::new(Face::Pz, 1, 0, 0);
    let meta = runtime.ensure_chunk_meta(key).unwrap();
    let camera = orbit_camera_state();
    let strategy = runtime.config.visibility_strategy;

    assert_eq!(
        runtime.horizon_visible(&camera, &meta),
        strategy.horizon_visible(&runtime.config, &camera, &meta)
    );
    assert_eq!(
        runtime.frustum_visible(&camera, &meta),
        strategy.frustum_visible(&runtime.config, &camera, &meta)
    );
    assert_eq!(
        runtime.projected_error_px(&camera, &meta),
        strategy.screen_error_px(&camera, &meta)
    );
}

#[test]
fn metadata_precompute_defaults_to_effective_runtime_max_lod() {
    let runtime = PlanetRuntime::new(
        RuntimeConfig {
            metadata_precompute_max_lod: 2,
            enable_godot_staging: false,
            ..RuntimeConfig::default()
        },
        Rid::Invalid,
        Rid::Invalid,
    );

    assert_eq!(
        runtime.metadata_precompute_max_lod(),
        runtime.config.max_lod
    );
    assert_eq!(runtime.meta_count(), 6 * (1 + 4 + 16 + 64 + 256 + 1024));
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
                .prepare_render_payload_request(sequence, sequence as u64 + 1, key, &desired_render)
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
    assert!(batch.queue_peak > 0);
    assert!(batch.result_wait_count <= batch.results.len());
}

#[test]
fn stale_async_payload_results_are_dropped_on_install() {
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
    let key = ChunkKey::new(Face::Pz, 2, 1, 1);
    let desired_render = [key].into_iter().collect::<HashSet<_>>();
    let request = runtime
        .prepare_render_payload_request(0, 1, key, &desired_render)
        .unwrap()
        .unwrap();
    let prepared = runtime.threaded_payload_generator.generate(vec![request]);
    let prepared = prepared.results.into_iter().next().unwrap();
    let mut frame_state = SelectionFrameState::default();

    runtime.pending_payload_requests.insert(
        key,
        PendingPayloadRequest {
            epoch: 2,
            surface_class: prepared.surface_class.clone(),
        },
    );

    assert!(!runtime.accept_prepared_render_payload(prepared, &mut frame_state));
    assert_eq!(frame_state.phase9_stale_results_dropped, 1);
    assert!(!runtime.resident_payloads.contains_key(&key));
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
fn rendered_chunk_edges_match_across_all_cross_face_seams() {
    let mut runtime = test_runtime();
    let last = mesh_topology::VISIBLE_VERTICES_PER_EDGE - 1;

    for face in Face::ALL {
        for edge in Edge::ALL {
            let key = boundary_key(face, 1, edge, 0);
            let neighbor = topology::same_lod_neighbor(key, edge).unwrap();
            let xform = topology::edge_transform(key.face, edge);

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
                let neighbor_position = neighbor_origin
                    + local_position_to_dvec3(neighbor_mesh.positions[neighbor_index]);

                assert!(
                    (chunk_position - neighbor_position).length() < 1.0e-3,
                    "seam mismatch face={face:?} edge={edge:?} step={step}: {chunk_position:?} vs {neighbor_position:?}"
                );
            }
        }
    }
}

#[test]
fn stitched_fine_edges_match_coarse_cover_for_delta_one_neighbors() {
    let mut runtime = test_runtime();
    let cases = [
        (ChunkKey::new(Face::Px, 2, 2, 1), Edge::NegU),
        (ChunkKey::new(Face::Px, 2, 1, 1), Edge::PosU),
        (ChunkKey::new(Face::Px, 2, 1, 2), Edge::NegV),
        (ChunkKey::new(Face::Px, 2, 1, 1), Edge::PosV),
    ];
    let last = mesh_topology::VISIBLE_VERTICES_PER_EDGE - 1;

    for (fine_key, edge) in cases {
        let neighbor_same_lod = topology::same_lod_neighbor(fine_key, edge).unwrap();
        let coarse_cover = neighbor_same_lod.parent().unwrap();
        let desired_render = [fine_key, coarse_cover].into_iter().collect::<HashSet<_>>();
        let fine_surface_class = runtime
            .required_surface_class_for_selection(fine_key, &desired_render)
            .unwrap();

        assert_eq!(
            fine_surface_class.stitch_mask,
            mesh_topology::stitch_mask_bit(edge)
        );

        let fine_origin = runtime
            .ensure_chunk_meta(fine_key)
            .unwrap()
            .bounds
            .center_planet;
        let fine_samples = runtime.sample_chunk_scalar_field(fine_key).unwrap();
        let fine_mesh = runtime
            .derive_cpu_mesh_buffers(&fine_samples, fine_surface_class.stitch_mask, fine_origin)
            .unwrap();

        let coarse_origin = runtime
            .ensure_chunk_meta(coarse_cover)
            .unwrap()
            .bounds
            .center_planet;
        let coarse_samples = runtime.sample_chunk_scalar_field(coarse_cover).unwrap();
        let coarse_mesh = runtime
            .derive_cpu_mesh_buffers(
                &coarse_samples,
                mesh_topology::BASE_STITCH_MASK,
                coarse_origin,
            )
            .unwrap();

        for step in (0..=last).step_by(2) {
            let fine_index = visible_edge_vertex_index(edge, step);
            let coarse_index = visible_edge_vertex_index(
                opposite_edge(edge),
                coarse_step_for_cover(fine_key, edge, step),
            );
            let fine_position =
                fine_origin + local_position_to_dvec3(fine_mesh.positions[fine_index]);
            let coarse_position =
                coarse_origin + local_position_to_dvec3(coarse_mesh.positions[coarse_index]);

            assert!(
                (fine_position - coarse_position).length() < 1.0e-3,
                "delta-1 seam mismatch fine={fine_key:?} coarse={coarse_cover:?} edge={edge:?} step={step}: {fine_position:?} vs {coarse_position:?}"
            );
        }

        for step in (1..last).step_by(2) {
            let fine_index = visible_edge_vertex_index(edge, step);
            assert!(
                !stitched_edge_uses_vertex(&fine_mesh, fine_index),
                "stitched edge kept odd boundary vertex fine={fine_key:?} edge={edge:?} step={step}"
            );
        }
    }
}

#[test]
fn seam_debug_snapshot_reports_active_and_pooled_stitch_masks() {
    let key_a = ChunkKey::new(Face::Px, 2, 1, 1);
    let key_b = ChunkKey::new(Face::Py, 2, 1, 1);
    let key_c = ChunkKey::new(Face::Pz, 2, 1, 1);
    let key_d = ChunkKey::new(Face::Nz, 2, 1, 1);
    let base_surface_class = SurfaceClassKey::canonical_chunk(
        mesh_topology::BASE_STITCH_MASK,
        DEFAULT_RENDER_MATERIAL_CLASS,
        DEFAULT_RENDER_FORMAT_MASK,
        DEFAULT_RENDER_VERTEX_STRIDE,
        DEFAULT_RENDER_ATTRIBUTE_STRIDE,
        DEFAULT_RENDER_INDEX_STRIDE,
    )
    .unwrap();
    let pos_u_surface_class = SurfaceClassKey::canonical_chunk(
        mesh_topology::stitch_mask_bit(Edge::PosU),
        DEFAULT_RENDER_MATERIAL_CLASS,
        DEFAULT_RENDER_FORMAT_MASK,
        DEFAULT_RENDER_VERTEX_STRIDE,
        DEFAULT_RENDER_ATTRIBUTE_STRIDE,
        DEFAULT_RENDER_INDEX_STRIDE,
    )
    .unwrap();
    let neg_u_neg_v_surface_class = SurfaceClassKey::canonical_chunk(
        mesh_topology::stitch_mask_bit(Edge::NegU) | mesh_topology::stitch_mask_bit(Edge::NegV),
        DEFAULT_RENDER_MATERIAL_CLASS,
        DEFAULT_RENDER_FORMAT_MASK,
        DEFAULT_RENDER_VERTEX_STRIDE,
        DEFAULT_RENDER_ATTRIBUTE_STRIDE,
        DEFAULT_RENDER_INDEX_STRIDE,
    )
    .unwrap();

    let mut runtime = test_runtime();
    runtime.active_render.extend([key_a, key_b, key_c, key_d]);
    runtime.install_render_entry(
        key_a,
        Rid::new(1),
        Rid::new(2),
        base_surface_class.clone(),
        None,
    );
    runtime.insert_payload(
        key_b,
        ChunkPayload {
            surface_class: pos_u_surface_class.clone(),
            ..sample_payload(&pos_u_surface_class, 1)
        },
    );
    runtime.install_render_entry(
        key_c,
        Rid::new(3),
        Rid::new(4),
        base_surface_class.clone(),
        None,
    );
    runtime.insert_payload(
        key_c,
        ChunkPayload {
            surface_class: neg_u_neg_v_surface_class.clone(),
            ..sample_payload(&neg_u_neg_v_surface_class, 2)
        },
    );
    runtime.push_render_pool_entry(RenderPoolEntry {
        mesh_rid: Rid::new(5),
        render_instance_rid: Rid::new(6),
        surface_class: pos_u_surface_class.clone(),
        gd_staging: None,
    });
    runtime.push_render_pool_entry(RenderPoolEntry {
        mesh_rid: Rid::new(7),
        render_instance_rid: Rid::new(8),
        surface_class: neg_u_neg_v_surface_class.clone(),
        gd_staging: None,
    });

    let seam = runtime.seam_debug_snapshot();

    assert_eq!(seam.active_render_chunks, 4);
    assert_eq!(seam.active_chunks_with_surface_class, 3);
    assert_eq!(seam.active_chunks_missing_surface_class, 1);
    assert_eq!(seam.active_stitched_chunks, 1);
    assert_eq!(seam.pending_surface_class_mismatch_chunks, 1);
    assert_eq!(
        seam.active_stitch_mask_counts[mesh_topology::BASE_STITCH_MASK as usize],
        2
    );
    assert_eq!(
        seam.active_stitch_mask_counts[mesh_topology::stitch_mask_bit(Edge::PosU) as usize],
        1
    );
    assert_eq!(seam.active_stitched_edge_counts[1], 1);
    assert_eq!(seam.pooled_render_entries, 2);
    assert_eq!(
        seam.pooled_stitch_mask_counts[mesh_topology::stitch_mask_bit(Edge::PosU) as usize],
        1
    );
    assert_eq!(
        seam.pooled_stitch_mask_counts[(mesh_topology::stitch_mask_bit(Edge::NegU)
            | mesh_topology::stitch_mask_bit(Edge::NegV))
            as usize],
        1
    );
    assert_eq!(seam.active_stitch_mask_summary(), "0:2|2:1");
    assert_eq!(seam.pooled_stitch_mask_summary(), "2:1|5:1");
    assert_eq!(
        seam.active_stitched_edge_summary(),
        "neg_u:0|pos_u:1|neg_v:0|pos_v:0"
    );
}

#[test]
fn phase12_chunk_asset_placement_replays_for_same_seed() {
    let config = RuntimeConfig {
        planet_seed: 42,
        ..RuntimeConfig::default()
    };
    let key = (0..=3)
        .flat_map(|y| (0..=3).map(move |x| ChunkKey::new(Face::Px, 3, x, y)))
        .find(|key| !build_chunk_asset_placement(&config, *key).assets.is_empty())
        .expect("phase 12 test should find at least one populated chunk");

    let a = build_chunk_asset_placement(&config, key);
    let b = build_chunk_asset_placement(&config, key);
    let c = build_chunk_asset_placement(
        &RuntimeConfig {
            planet_seed: 43,
            ..config.clone()
        },
        key,
    );

    assert_eq!(a, b);
    assert!(a.candidate_count > 0);
    assert!(!a.assets.is_empty());
    assert!(a.assets.iter().all(|asset| asset.scale > 0.0));
    assert_ne!(a.assets, c.assets);
}

#[test]
fn phase12_asset_grouping_stays_compact_within_chunk_batches() {
    let span = DEFAULT_ASSET_GROUP_CHUNK_SPAN;
    let key_a = ChunkKey::new(Face::Px, 3, 0, 0);
    let key_b = ChunkKey::new(Face::Px, 3, 1, 1);
    let key_c = ChunkKey::new(Face::Px, 3, span, 0);

    let group_a = asset_group_key_for_chunk(key_a, 0, span);
    let group_b = asset_group_key_for_chunk(key_b, 0, span);
    let group_c = asset_group_key_for_chunk(key_c, 0, span);

    assert_eq!(group_a, group_b);
    assert_ne!(group_a, group_c);
}

#[test]
fn phase12_asset_residency_follows_active_render_chunks() {
    let mut runtime = test_runtime();
    let keys = [
        ChunkKey::new(Face::Px, 3, 0, 0),
        ChunkKey::new(Face::Px, 3, 1, 0),
        ChunkKey::new(Face::Px, 3, 2, 0),
    ];

    for (fill, key) in keys.into_iter().enumerate() {
        let meta = runtime.ensure_chunk_meta(key).unwrap();
        runtime.insert_payload(
            key,
            ChunkPayload {
                surface_class: meta.surface_class.clone(),
                chunk_origin_planet: meta.bounds.center_planet,
                assets: build_chunk_asset_placement(&runtime.config, key).assets,
                ..sample_payload(&meta.surface_class, fill as u8)
            },
        );
    }

    runtime.active_render.extend([keys[0], keys[1]]);
    runtime.asset_groups_dirty = true;
    let mut frame_state = SelectionFrameState::default();
    for _ in 0..8 {
        runtime.sync_asset_groups(&mut frame_state).unwrap();
        if runtime.pending_asset_group_epoch.is_none() && !runtime.asset_groups_dirty {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let expected_before = build_desired_asset_groups(
        &runtime.config,
        &runtime.active_render,
        &runtime.resident_payloads,
        &runtime.meta,
    );

    assert_eq!(runtime.active_asset_group_count(), expected_before.len());
    assert_eq!(
        runtime.active_asset_instance_count(),
        expected_before
            .values()
            .map(|group| group.assets.len())
            .sum::<usize>()
    );
    assert_eq!(asset_group_summary(&runtime).len(), expected_before.len());

    runtime.active_render.remove(&keys[0]);
    runtime.asset_groups_dirty = true;
    for _ in 0..8 {
        runtime.sync_asset_groups(&mut frame_state).unwrap();
        if runtime.pending_asset_group_epoch.is_none() && !runtime.asset_groups_dirty {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let expected_after = build_desired_asset_groups(
        &runtime.config,
        &runtime.active_render,
        &runtime.resident_payloads,
        &runtime.meta,
    );

    assert_eq!(runtime.active_asset_group_count(), expected_after.len());
    assert_eq!(
        runtime.active_asset_instance_count(),
        expected_after
            .values()
            .map(|group| group.assets.len())
            .sum::<usize>()
    );

    runtime.active_render.clear();
    runtime.asset_groups_dirty = true;
    for _ in 0..8 {
        runtime.sync_asset_groups(&mut frame_state).unwrap();
        if runtime.pending_asset_group_epoch.is_none() && !runtime.asset_groups_dirty {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(runtime.active_asset_group_count(), 0);
    assert_eq!(runtime.active_asset_instance_count(), 0);
}

#[test]
fn phase12_camera_path_preserves_valid_asset_residency_under_async_streaming() {
    let config = RuntimeConfig {
        metadata_precompute_max_lod: 0,
        enable_godot_staging: false,
        planet_seed: 7,
        ..RuntimeConfig::default()
    };
    let mut runtime_a = PlanetRuntime::new(config.clone(), Rid::Invalid, Rid::Invalid);
    let mut runtime_b = PlanetRuntime::new(config, Rid::Invalid, Rid::Invalid);
    let path = [
        orbit_camera_state(),
        CameraState {
            position_planet: DVec3::new(450.0, 120.0, 1_650.0),
            ..orbit_camera_state()
        },
        near_surface_camera_state(),
        CameraState {
            position_planet: DVec3::new(220.0, 90.0, 1_120.0),
            ..near_surface_camera_state()
        },
    ];

    for camera in path {
        step_runtime_until_streaming_settles(&mut runtime_a, &camera, 24);
        step_runtime_until_streaming_settles(&mut runtime_b, &camera, 24);
    }

    let snapshot_a = runtime_a.asset_debug_snapshot();
    let snapshot_b = runtime_b.asset_debug_snapshot();
    assert!(snapshot_a.active_groups == 0 || snapshot_a.active_instances > 0);
    assert!(snapshot_b.active_groups == 0 || snapshot_b.active_instances > 0);
    assert!(snapshot_a.active_groups == 0 || snapshot_a.family_meshes > 0);
    assert!(snapshot_b.active_groups == 0 || snapshot_b.family_meshes > 0);
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
fn frustum_culling_can_be_disabled_in_runtime_config() {
    let runtime = PlanetRuntime::new(
        RuntimeConfig {
            enable_frustum_culling: false,
            ..RuntimeConfig::default()
        },
        Rid::Invalid,
        Rid::Invalid,
    );
    let meta = ChunkMeta::new(
        sample_key(),
        ChunkBounds::new(
            DVec3::new(10_000.0, 0.0, 0.0),
            10.0,
            -10.0,
            10.0,
            990.0,
            1_010.0,
        ),
        ChunkMetrics::new(1.0, 0.0, 0.05),
        sample_surface_class(),
    )
    .unwrap();
    let camera = CameraState {
        position_planet: DVec3::new(0.0, 0.0, 2_000.0),
        forward_planet: DVec3::new(0.0, 0.0, -1.0),
        frustum_planes: box_test_frustum(Vector3::ZERO, 100.0),
        projection_scale: 1_200.0,
        origin: OriginSnapshot::for_config(&runtime.config, DVec3::ZERO),
    };

    assert!(runtime.frustum_visible(&camera, &meta));
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

    let first_requests = keys
        .iter()
        .copied()
        .enumerate()
        .map(|(sequence, key)| {
            runtime
                .prepare_render_payload_request(sequence, sequence as u64 + 1, key, &desired_render)
                .unwrap()
                .unwrap()
        })
        .collect::<Vec<_>>();
    let _ = runtime.threaded_payload_generator.generate(first_requests);

    let second_requests = keys
        .iter()
        .copied()
        .enumerate()
        .map(|(sequence, key)| {
            runtime
                .prepare_render_payload_request(
                    sequence,
                    sequence as u64 + 101,
                    key,
                    &desired_render,
                )
                .unwrap()
                .unwrap()
        })
        .collect::<Vec<_>>();
    let second_batch = runtime.threaded_payload_generator.generate(second_requests);

    assert_eq!(runtime.threaded_payload_generator.worker_count(), 1);
    assert_eq!(second_batch.results.len(), keys.len());
    assert!(second_batch.results.iter().any(|prepared| {
        prepared.scratch_metrics.sample_reuse
            || prepared.scratch_metrics.mesh_reuse
            || prepared.scratch_metrics.pack_reuse
    }));
}

#[test]
fn coverage_ready_requires_replacement_chunks_before_parent_retirement() {
    let parent = ChunkKey::new(Face::Pz, 2, 1, 1);
    let children = parent.children().unwrap();
    let desired_render = children.into_iter().collect::<HashSet<_>>();
    let ready_none = HashSet::new();
    let ready_partial = [children[0], children[1]]
        .into_iter()
        .collect::<HashSet<_>>();
    let ready_all = children.into_iter().collect::<HashSet<_>>();

    assert!(!PlanetRuntime::coverage_ready_for_key(
        parent,
        &desired_render,
        &ready_none
    ));
    assert!(!PlanetRuntime::coverage_ready_for_key(
        parent,
        &desired_render,
        &ready_partial
    ));
    assert!(PlanetRuntime::coverage_ready_for_key(
        parent,
        &desired_render,
        &ready_all
    ));
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

    let stored = runtime.ensure_chunk_meta(key).unwrap();
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
fn ensure_chunk_meta_returns_prebuilt_bounds_and_surface_class() {
    let mut runtime = test_runtime();
    let key = ChunkKey::new(Face::Pz, 3, 5, 2);
    let initial_meta_count = runtime.meta_count();

    let meta = runtime.ensure_chunk_meta(key).unwrap();

    assert_eq!(meta.key, key);
    assert_eq!(runtime.meta_count(), initial_meta_count);
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
    let camera = near_surface_camera_state();

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
fn coarse_lod_fallback_keeps_root_chunk_when_face_is_fully_culled() {
    let base_config = RuntimeConfig {
        max_lod_policy: MaxLodPolicyKind::Fixed,
        max_lod: 1,
        metadata_precompute_max_lod: 1,
        payload_precompute_max_lod: 1,
        split_threshold_px: 0.0,
        merge_threshold_px: 0.0,
        enable_godot_staging: false,
        ..RuntimeConfig::default()
    };
    let mut runtime = PlanetRuntime::new(base_config.clone(), Rid::Invalid, Rid::Invalid);
    let root = ChunkKey::new(Face::Pz, 0, 0, 0);
    let camera = CameraState {
        position_planet: DVec3::new(0.0, 0.0, 30_000.0),
        forward_planet: DVec3::new(0.0, 0.0, -1.0),
        frustum_planes: box_test_frustum(Vector3::new(50_000.0, 50_000.0, 50_000.0), 100.0),
        projection_scale: 1_200.0,
        origin: OriginSnapshot::for_config(&runtime.config, DVec3::ZERO),
    };
    let mut frame_state = SelectionFrameState::default();
    let selected_without_fallback = runtime
        .select_render_set(&camera, &mut frame_state)
        .unwrap();

    let mut fallback_runtime = PlanetRuntime::new(
        RuntimeConfig {
            keep_coarse_lod_chunks_rendered: true,
            ..base_config
        },
        Rid::Invalid,
        Rid::Invalid,
    );
    let mut fallback_frame_state = SelectionFrameState::default();
    let selected_with_fallback = fallback_runtime
        .select_render_set(&camera, &mut fallback_frame_state)
        .unwrap();

    assert!(!selected_without_fallback.contains(&root));
    assert!(selected_with_fallback.contains(&root));
}

#[test]
fn neighbor_normalization_splits_coarse_side_when_metadata_is_available() {
    let mut runtime = PlanetRuntime::new(
        RuntimeConfig {
            metadata_precompute_max_lod: 0,
            enable_godot_staging: false,
            ..RuntimeConfig::default()
        },
        Rid::Invalid,
        Rid::Invalid,
    );
    let fine_key = ChunkKey::new(Face::Px, 3, 3, 0);
    let coarse_cover = ChunkKey::new(Face::Px, 1, 1, 0);
    let mut selected = [fine_key, coarse_cover].into_iter().collect::<HashSet<_>>();

    let splits = runtime.normalize_neighbor_lod_delta(&mut selected).unwrap();

    assert_eq!(splits, 1);
    assert!(!selected.contains(&coarse_cover));
    assert!(selected.contains(&fine_key));
    assert_eq!(runtime.pending_meta_requests.len(), 0);
    assert!(selected
        .iter()
        .any(|key| key.lod == 2 && key.is_descendant_of(&coarse_cover)));
    assert!(runtime
        .required_surface_class_for_selection(fine_key, &selected)
        .is_ok());
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

    for _ in 0..8 {
        runtime.step_visibility_selection(&camera).unwrap();
        if runtime.deferred_commit_count() > 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }

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

    for _ in 0..8 {
        runtime.step_visibility_selection(&camera).unwrap();
        if runtime.active_render_count() > 0 || runtime.deferred_commit_count() > 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }

    assert!(runtime.active_render_count() <= 1);
    assert_eq!(runtime.active_physics_count(), 0);
    assert!(runtime.desired_render_count() > runtime.active_render_count());
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

    step_runtime_until_streaming_settles(&mut runtime, &camera, 40);

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
fn flush_pending_origin_rebinds_rebinds_active_transforms_immediately() {
    let mut runtime = PlanetRuntime::new(
        RuntimeConfig {
            metadata_precompute_max_lod: 0,
            enable_godot_staging: false,
            ..RuntimeConfig::default()
        },
        Rid::Invalid,
        Rid::Invalid,
    );
    let key = sample_key();
    let surface_class = sample_surface_class();
    let mut payload = sample_payload(&surface_class, 9);
    payload.chunk_origin_planet = DVec3::new(4.0, 5.0, 6.0);
    runtime.insert_payload(key, payload);
    runtime.active_render.insert(key);
    runtime.active_physics.insert(key);

    assert!(runtime.update_origin_from_camera(DVec3::new(2_048.0, 0.0, 0.0)));
    runtime.flush_pending_origin_rebinds();

    assert!(!runtime.origin_shift_pending_rebind);
    assert_eq!(runtime.frame_state.phase10_origin_rebases, 1);
    assert_eq!(runtime.frame_state.phase10_render_transform_rebinds, 1);
    assert_eq!(runtime.frame_state.phase10_physics_transform_rebinds, 1);
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

#[test]
fn phase15_render_backend_can_be_exercised_independently() {
    let mut runtime = test_runtime();
    let key = ChunkKey::new(Face::Pz, 2, 1, 1);
    let desired_render = [key].into_iter().collect::<HashSet<_>>();
    let mut frame_state = SelectionFrameState::default();

    runtime
        .ensure_render_payload_for_selection(key, &desired_render, &mut frame_state)
        .unwrap();
    frame_state = SelectionFrameState::default();

    let backend = runtime.config.render_backend;
    assert!(backend.commit_render_payload(&mut runtime, key, &mut frame_state));
    assert_eq!(frame_state.phase8_render_cold_commits, 1);
    assert!(runtime.ensure_rid_state(key).render_resident);

    backend.deactivate_render(&mut runtime, key);
    assert!(!runtime.ensure_rid_state(key).render_resident);
}
