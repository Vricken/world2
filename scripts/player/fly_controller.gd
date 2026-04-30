extends CharacterBody3D

@export var move_speed: float = 150.0
@export var min_speed: float = 18.75
@export var max_speed: float = 96000.0
@export var mouse_sensitivity: float = 0.0025

@onready var pitch_pivot: Node3D = $PitchPivot

var _yaw: float = 0.0
var _pitch: float = 0.0

func _ready() -> void:
	_ensure_default_actions()
	motion_mode = CharacterBody3D.MOTION_MODE_FLOATING
	_yaw = rotation.y
	_pitch = pitch_pivot.rotation.x
	Input.set_mouse_mode(Input.MOUSE_MODE_CAPTURED)
	print(
		"FlyController ready. Controls: WASD move, Space/Shift vertical, mouse look, Up doubles speed, Down halves speed, Esc releases mouse."
	)
	print("FlyController speed: %.2f" % move_speed)


func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventMouseMotion and Input.get_mouse_mode() == Input.MOUSE_MODE_CAPTURED:
		_yaw -= event.relative.x * mouse_sensitivity
		_pitch = clamp(_pitch - event.relative.y * mouse_sensitivity, deg_to_rad(-89.0), deg_to_rad(89.0))
		rotation.y = _yaw
		pitch_pivot.rotation.x = _pitch
		return

	if event is InputEventMouseButton and event.pressed and event.button_index == MOUSE_BUTTON_LEFT:
		Input.set_mouse_mode(Input.MOUSE_MODE_CAPTURED)
		return

	if not (event is InputEventKey) or not event.pressed or event.echo:
		return

	if event.keycode == KEY_ESCAPE:
		Input.set_mouse_mode(Input.MOUSE_MODE_VISIBLE)
	elif event.is_action_pressed(&"fly_speed_up"):
		_set_move_speed(min(move_speed * 2.0, max_speed))
	elif event.is_action_pressed(&"fly_speed_down"):
		_set_move_speed(max(move_speed * 0.5, min_speed))


func _physics_process(_delta: float) -> void:
	var direction := Vector3.ZERO
	var view_basis := pitch_pivot.global_transform.basis
	var forward := -view_basis.z
	var right := view_basis.x
	var up := transform.basis.y

	direction += forward * Input.get_axis(&"fly_backward", &"fly_forward")
	direction += right * Input.get_axis(&"fly_left", &"fly_right")
	direction += up * Input.get_axis(&"fly_down", &"fly_up")

	if direction.length_squared() > 0.0:
		direction = direction.normalized()

	velocity = direction * move_speed
	move_and_slide()


func _set_move_speed(next_speed: float) -> void:
	move_speed = clamp(next_speed, min_speed, max_speed)
	print("FlyController speed: %.2f" % move_speed)


func _ensure_default_actions() -> void:
	_ensure_key_action(&"fly_forward", KEY_W)
	_ensure_key_action(&"fly_backward", KEY_S)
	_ensure_key_action(&"fly_left", KEY_A)
	_ensure_key_action(&"fly_right", KEY_D)
	_ensure_key_action(&"fly_up", KEY_SPACE)
	_ensure_key_action(&"fly_down", KEY_SHIFT)
	_ensure_key_action(&"fly_speed_up", KEY_UP)
	_ensure_key_action(&"fly_speed_down", KEY_DOWN)


func _ensure_key_action(action_name: StringName, physical_keycode: int) -> void:
	if not InputMap.has_action(action_name):
		InputMap.add_action(action_name)

	if _action_has_physical_key(action_name, physical_keycode):
		return

	var event := InputEventKey.new()
	event.physical_keycode = physical_keycode
	InputMap.action_add_event(action_name, event)


func _action_has_physical_key(action_name: StringName, physical_keycode: int) -> bool:
	for existing_event in InputMap.action_get_events(action_name):
		if existing_event is InputEventKey and existing_event.physical_keycode == physical_keycode:
			return true
	return false
