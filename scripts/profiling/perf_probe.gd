extends Node

const SMALL_WINDOW_FRACTION := 0.5
const SETTLE_SECONDS := 2.0
const SAMPLE_SECONDS := 6.0

@onready var world_root: Node = $World
@onready var planet_root: Node = $World
@onready var atmosphere: Node3D = $World/PlanetAtmosphere


func _ready() -> void:
	Input.set_mouse_mode(Input.MOUSE_MODE_VISIBLE)
	call_deferred("_run")


func _run() -> void:
	var scenario_name := _scenario_name()
	var root_window := get_window()
	var screen_size := DisplayServer.screen_get_size()
	var small_window_size := _small_window_size(screen_size)
	var window_mode := Window.MODE_WINDOWED
	var window_size := small_window_size
	var atmosphere_visible := true
	var scaling_3d_scale := 1.0

	match scenario_name:
		"small_window":
			pass
		"fullscreen_native":
			window_mode = Window.MODE_FULLSCREEN
		"fullscreen_native_no_atmosphere":
			window_mode = Window.MODE_FULLSCREEN
			atmosphere_visible = false
		_:
			push_error("Unknown perf probe scenario: %s" % scenario_name)
			get_tree().quit(2)
			return

	_apply_scenario(
		root_window,
		window_mode,
		window_size,
		atmosphere_visible,
		scaling_3d_scale
	)
	await get_tree().create_timer(SETTLE_SECONDS).timeout

	var result_line := await _sample_scenario(
		root_window,
		scenario_name,
		window_mode,
		atmosphere_visible
	)
	print(result_line)

	atmosphere.visible = true
	get_viewport().scaling_3d_scale = 1.0
	get_tree().quit()


func _scenario_name() -> String:
	for arg in OS.get_cmdline_user_args():
		if arg.begins_with("--scenario="):
			return arg.trim_prefix("--scenario=")
	return "small_window"


func _small_window_size(screen_size: Vector2i) -> Vector2i:
	return Vector2i(
		maxi(640, int(round(screen_size.x * SMALL_WINDOW_FRACTION))),
		maxi(360, int(round(screen_size.y * SMALL_WINDOW_FRACTION)))
	)


func _apply_scenario(
	root_window: Window,
	window_mode: int,
	window_size: Vector2i,
	atmosphere_is_visible: bool,
	scaling_3d_scale: float
) -> void:
	root_window.mode = window_mode

	if window_mode == Window.MODE_WINDOWED:
		root_window.size = window_size
		var screen_size := DisplayServer.screen_get_size()
		root_window.position = (screen_size - window_size) / 2

	get_viewport().scaling_3d_scale = scaling_3d_scale
	atmosphere.visible = atmosphere_is_visible


func _sample_scenario(
	root_window: Window,
	scenario_name: String,
	window_mode: int,
	atmosphere_is_visible: bool
) -> String:
	var frame_count := 0
	var fps_elapsed_start_usec := Time.get_ticks_usec()
	var start_tick := int(planet_root.call("runtime_tick_count"))
	var start_payloads := int(planet_root.call("runtime_resident_payload_count"))
	var start_active_render := int(planet_root.call("runtime_active_render_count"))
	var start_desired_render := int(planet_root.call("runtime_desired_render_count"))

	var desired_render_sum := 0.0
	var active_render_sum := 0.0
	var resident_payload_sum := 0.0
	var deferred_commit_sum := 0.0
	var deferred_upload_bytes_sum := 0.0
	var viewport_height_sum := 0.0
	var window_height_sum := 0.0
	var selected_candidates_sum := 0.0
	var refinement_iterations_sum := 0.0
	var selection_cap_hits_sum := 0.0
	var render_residency_sum := 0.0
	var render_residency_evictions_sum := 0.0
	var render_tile_bytes_sum := 0.0
	var render_tile_pool_slots_sum := 0.0
	var render_tile_pool_active_slots_sum := 0.0
	var render_tile_pool_free_slots_sum := 0.0
	var render_tile_eviction_ready_slots_sum := 0.0
	var selected_render_starved_sum := 0.0
	var selected_render_starvation_failures_sum := 0.0
	var max_selected_render_starvation_frames_sum := 0.0

	while (Time.get_ticks_usec() - fps_elapsed_start_usec) < int(SAMPLE_SECONDS * 1_000_000.0):
		await get_tree().process_frame
		frame_count += 1

		desired_render_sum += float(planet_root.call("runtime_desired_render_count"))
		active_render_sum += float(planet_root.call("runtime_active_render_count"))
		resident_payload_sum += float(planet_root.call("runtime_resident_payload_count"))
		deferred_commit_sum += float(planet_root.call("runtime_deferred_commit_count"))
		deferred_upload_bytes_sum += float(planet_root.call("runtime_deferred_upload_bytes"))
		viewport_height_sum += get_viewport().get_visible_rect().size.y
		window_height_sum += root_window.size.y
		selected_candidates_sum += float(planet_root.call("runtime_selected_candidates"))
		refinement_iterations_sum += float(planet_root.call("runtime_refinement_iterations"))
		selection_cap_hits_sum += float(planet_root.call("runtime_selection_cap_hits"))
		render_residency_sum += float(planet_root.call("runtime_render_residency_count"))
		render_residency_evictions_sum += float(planet_root.call("runtime_render_residency_evictions"))
		render_tile_bytes_sum += float(planet_root.call("runtime_render_tile_bytes"))
		render_tile_pool_slots_sum += float(planet_root.call("runtime_render_tile_pool_slots"))
		render_tile_pool_active_slots_sum += float(
			planet_root.call("runtime_render_tile_pool_active_slots")
		)
		render_tile_pool_free_slots_sum += float(
			planet_root.call("runtime_render_tile_pool_free_slots")
		)
		render_tile_eviction_ready_slots_sum += float(
			planet_root.call("runtime_render_tile_eviction_ready_slots")
		)
		selected_render_starved_sum += float(planet_root.call("runtime_selected_render_starved_chunks"))
		selected_render_starvation_failures_sum += float(
			planet_root.call("runtime_selected_render_starvation_failures")
		)
		max_selected_render_starvation_frames_sum += float(
			planet_root.call("runtime_max_selected_render_starvation_frames")
		)

	var elapsed_seconds: float = maxf(
		float(Time.get_ticks_usec() - fps_elapsed_start_usec) / 1_000_000.0,
		0.001
	)
	var end_tick := int(planet_root.call("runtime_tick_count"))
	var end_payloads := int(planet_root.call("runtime_resident_payload_count"))
	var end_active_render := int(planet_root.call("runtime_active_render_count"))
	var end_desired_render := int(planet_root.call("runtime_desired_render_count"))

	var viewport_size := get_viewport().get_visible_rect().size
	var fields := PackedStringArray()
	fields.append("scenario=%s" % scenario_name)
	fields.append("window_mode=%d" % window_mode)
	fields.append("window_width_points=%d" % root_window.size.x)
	fields.append("window_height_points=%d" % root_window.size.y)
	fields.append("viewport_width_px=%d" % int(viewport_size.x))
	fields.append("viewport_height_px=%d" % int(viewport_size.y))
	fields.append("scaling_3d_scale=%.4f" % float(get_viewport().scaling_3d_scale))
	fields.append("atmosphere_visible=%s" % str(atmosphere_is_visible))
	fields.append("avg_fps=%.4f" % (frame_count / elapsed_seconds))
	fields.append("process_ticks_per_second=%.4f" % (float(end_tick - start_tick) / elapsed_seconds))
	fields.append("avg_desired_render=%.4f" % (desired_render_sum / max(frame_count, 1)))
	fields.append("avg_active_render=%.4f" % (active_render_sum / max(frame_count, 1)))
	fields.append("avg_resident_payloads=%.4f" % (resident_payload_sum / max(frame_count, 1)))
	fields.append("avg_deferred_commits=%.4f" % (deferred_commit_sum / max(frame_count, 1)))
	fields.append(
		"avg_selected_candidates=%.4f" %
		(selected_candidates_sum / max(frame_count, 1))
	)
	fields.append(
		"avg_refinement_iterations=%.4f" %
		(refinement_iterations_sum / max(frame_count, 1))
	)
	fields.append(
		"avg_selection_cap_hits=%.4f" %
		(selection_cap_hits_sum / max(frame_count, 1))
	)
	fields.append(
		"avg_render_residency=%.4f" %
		(render_residency_sum / max(frame_count, 1))
	)
	fields.append(
		"avg_render_residency_evictions=%.4f" %
		(render_residency_evictions_sum / max(frame_count, 1))
	)
	fields.append(
		"avg_render_tile_mib=%.6f" %
		((render_tile_bytes_sum / max(frame_count, 1)) / 1048576.0)
	)
	fields.append(
		"avg_render_tile_pool_slots=%.4f" %
		(render_tile_pool_slots_sum / max(frame_count, 1))
	)
	fields.append(
		"avg_render_tile_pool_active_slots=%.4f" %
		(render_tile_pool_active_slots_sum / max(frame_count, 1))
	)
	fields.append(
		"avg_render_tile_pool_free_slots=%.4f" %
		(render_tile_pool_free_slots_sum / max(frame_count, 1))
	)
	fields.append(
		"avg_render_tile_eviction_ready_slots=%.4f" %
		(render_tile_eviction_ready_slots_sum / max(frame_count, 1))
	)
	fields.append(
		"avg_selected_render_starved=%.4f" %
		(selected_render_starved_sum / max(frame_count, 1))
	)
	fields.append(
		"avg_selected_render_starvation_failures=%.4f" %
		(selected_render_starvation_failures_sum / max(frame_count, 1))
	)
	fields.append(
		"avg_selected_render_starvation_frames=%.4f" %
		(max_selected_render_starvation_frames_sum / max(frame_count, 1))
	)
	fields.append(
		"avg_deferred_upload_mib=%.6f" %
		((deferred_upload_bytes_sum / max(frame_count, 1)) / 1048576.0)
	)
	fields.append("avg_viewport_height_px=%.4f" % (viewport_height_sum / max(frame_count, 1)))
	fields.append("avg_window_height_points=%.4f" % (window_height_sum / max(frame_count, 1)))
	fields.append(
		"selection_reference_height_px=%.2f" %
		float(planet_root.call("runtime_render_lod_reference_height_px"))
	)
	fields.append("target_render_chunks=%d" % int(planet_root.call("runtime_target_render_chunks")))
	fields.append("hard_render_chunk_cap=%d" % int(planet_root.call("runtime_hard_render_chunk_cap")))
	fields.append(
		"fullscreen_lod_bias=%s" %
		str(planet_root.call("runtime_fullscreen_lod_bias")).replace(" ", "|")
	)
	fields.append("payloads_per_second=%.4f" % (float(end_payloads - start_payloads) / elapsed_seconds))
	fields.append(
		"active_render_per_second=%.4f" %
		(float(end_active_render - start_active_render) / elapsed_seconds)
	)
	fields.append(
		"desired_render_per_second=%.4f" %
		(float(end_desired_render - start_desired_render) / elapsed_seconds)
	)
	fields.append(
		"strategy_summary=%s" %
		str(planet_root.call("runtime_strategy_summary")).replace(" ", "|")
	)
	return "PERF_RESULT %s" % " ".join(fields)
