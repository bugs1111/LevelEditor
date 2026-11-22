@tool

const Util = preload("util.gd")
const TileUtil = preload("tile-util.gd")

static var hitUnresolved := false
static var current_field_name: String = "N/A"

static func create_fields(fields: Array, entity: Variant = null) -> Dictionary:
	var dict := {}
	for field in fields:
		var key: String = field.__identifier
		current_field_name = key
		dict[key] = parse_field(field)

		if not Util.options.resolve_entityrefs:
			hitUnresolved = false
			continue

		if hitUnresolved and entity != null:
			if dict[key] is Array:
				for index in range(dict[key].size()):
					Util.add_unresolved_reference(dict[key], index, entity)
			else:
				Util.add_unresolved_reference(dict, key, entity)
		hitUnresolved = false

	return dict

static func parse_field(field: Dictionary) -> Variant:
	var value: Variant = field.__value
	if value == null:
		return null

	var type := field.__type as String
	if type.begins_with("Array<"):
		var element_type := type.substr(6, type.length() - 7)
		var elements: Array = value.map(func(item): return parse_field({"__type": element_type, "__value": item}))
		if element_type.begins_with("LocalEnum.") and not elements.has(null):
			var enums: Array[String] = []
			enums.assign(elements)
			return enums
		return elements

	# Handle enum string
	var localEnum: String
	if type.contains("LocalEnum"):
		var regex = RegEx.new()
		regex.compile("(?<=\\.)\\w+")
		localEnum = regex.search(type).get_string()

		type = "LocalEnum"

	# Match field type
	match type:
		"Int":
			return int(value) as int
		"Color":
			return Color.from_string(value, Color.MAGENTA) as Color
		"Point":
			return __parse_point(value.cx, value.cy) as Vector2i
		"Tile":
			return __parse_tile(value) as AtlasTexture
		"EntityRef":
			hitUnresolved = true
			return value.entityIid as String
		"LocalEnum":
			return __parse_enum(localEnum, value) as String
		_:
			return value

static func __parse_point(x: int, y: int) -> Vector2i:
	# NOTE: would convert gridcoords to pixelcoords here, but needs more data
	# LDTKEntity currently converts it using LayerDefinition.
	return Vector2i(x,y)

static func __parse_enum(enum_str: String, value: String) -> String:
	var result: String = "%s.%s" % [enum_str, value]
	return result

static func __parse_tile(value: Dictionary) -> AtlasTexture:
	var texture := AtlasTexture.new()
	var atlas: TileSetAtlasSource

	if Util.tileset_refs.has(int(value.tilesetUid)):
		atlas = Util.tileset_refs[int(value.tilesetUid)]

	if atlas == null :
		print("Tileset Refs: ", Util.tileset_refs)
		print("FieldDef<Tile> '%s' could not find Tileset with UID '%s'. Using empty texture instead. Please fix this in your LDtk file." % [current_field_name, value.tilesetUid])
		return texture

	texture.atlas = atlas.texture

	var coords = TileUtil.px_to_grid(
			Vector2(value.x, value.y),
			atlas.texture_region_size,
			atlas.margins,
			atlas.separation
	)
	texture.region = atlas.get_tile_texture_region(coords) as Rect2i
	return texture
