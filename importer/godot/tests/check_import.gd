extends SceneTree

var failures: Array[String] = []


func _initialize() -> void:
	call_deferred("_run")


func check(condition: bool, message: String) -> void:
	if not condition:
		failures.append(message)
		printerr("FAIL: ", message)


func _run() -> void:
	var expected: Dictionary = JSON.parse_string(FileAccess.get_file_as_string(OS.get_cmdline_user_args()[0]))
	var packed = load("res://tests/maps/smoke.ldtk")
	check(packed is PackedScene, "LDtk imports as a PackedScene")
	if not packed is PackedScene:
		quit(1)
		return
	var world: Node = packed.instantiate()
	root.add_child(world)
	if expected.multi_world:
		check(world.get_child_count() == 2, "all worlds are imported")
		check(world.get_child(0).name == "First" and world.get_child(1).name == "Second", "world order matches the source")
		check(world.find_child("Other", true, false) != null, "second world's level is imported")
		if expected.get("override", false):
			var other := world.find_child("Other", true, false)
			var other_ground := other.find_child("Ground", true, false) as TileMapLayer
			check(other_ground.get_used_cells().size() == 2, "multi-world override retains all tiles")
			check(other_ground.get_cell_source_id(Vector2i.ZERO) == 60, "multi-world layer uses its override tileset")
			check(other_ground.get_cell_atlas_coords(Vector2i(1, 0)) == Vector2i(1, 0), "override uses its own grid size")
	var level: Node = world.find_child("Smoke", true, false)
	check(level != null, "imported level exists")
	if level == null:
		world.free()
		quit(1)
		return
	check(level.get("world_position") == Vector2(80, 32), "level world position is preserved")
	var ground := level.find_child("Ground", true, false) as TileMapLayer
	check(ground != null, "tiles import as TileMapLayer")
	if ground != null:
		check(ground.get_used_cells().size() == expected.tile_count, "tile count matches current source")
		check(ground.get_cell_source_id(Vector2i(0, 0)) == 3, "tile uses the source tileset")
		check(ground.get_cell_atlas_coords(Vector2i(0, 0)) == Vector2i.ZERO, "tile atlas coordinate is preserved")
		if expected.tile_count == 2:
			check(ground.get_cell_atlas_coords(Vector2i(1, 0)) == Vector2i(1, 0), "second tile uses its atlas coordinate")
			check((ground.get_cell_alternative_tile(Vector2i(1, 0)) & TileSetAtlasSource.TRANSFORM_FLIP_H) != 0, "horizontal tile flip is preserved")
	var entities: Node = level.find_child("Entities", true, false)
	check(entities != null, "entity layer exists")
	if entities != null:
		check(entities.get_meta("enum_array_builtin", -1) == TYPE_STRING, "Post-Import receives Array[String]")
		check(entities.get_meta("empty_enum_array_builtin", -1) == TYPE_STRING, "Post-Import receives empty Array[String]")
		var values: Array = entities.get("entities")
		check(values.size() == 1, "entity count is preserved")
		if values.size() == 1:
			var entity: Dictionary = values[0]
			check(entity.identifier == "PlayerSpawn", "entity identifier is preserved")
			check(entity.position == Vector2i(16, 16), "entity position remains relative to the level")
			check(entity.fields.health == expected.health, "entity field uses the current source value")
			check(typeof(entity.fields.health) == TYPE_INT, "Int fields become Godot integers")
			check(entity.fields.note == null, "nullable scalar field is preserved")
			check(entity.fields.route == [null, Vector2i(1, 1)], "nullable Point array is preserved")
			check(entity.fields.inventory == [1, 2] and typeof(entity.fields.inventory[0]) == TYPE_INT, "Int arrays contain integers")
			check(entity.fields.colors == [null, Color.RED], "nullable Color array is preserved")
			check(entity.fields.tiles.size() == 2 and entity.fields.tiles[0] == null and entity.fields.tiles[1] is AtlasTexture, "nullable Tile array is preserved")
			check(entity.fields.references == [null], "nullable EntityRef array is preserved")
			check(entity.fields.roles == ["Role.Hero", "Role.Guard"], "enum array values are preserved")
			check(entity.fields.empty_roles.is_empty(), "empty enum array is preserved")
	world.free()
	if failures.is_empty():
		print("PASS: LDtk PackedScene, TileMapLayer, atlas, flip, entity position and typed fields")
	quit(0 if failures.is_empty() else 1)
