@tool
extends RefCounted


func post_import(layer: LDTKEntityLayer) -> LDTKEntityLayer:
	var fields: Dictionary = layer.entities[0].fields
	assert(fields.roles.get_typed_builtin() == TYPE_STRING, "enum arrays retain Array[String] during Post-Import")
	assert(fields.empty_roles.get_typed_builtin() == TYPE_STRING, "empty enum arrays retain Array[String] during Post-Import")
	layer.set_meta("enum_array_builtin", fields.roles.get_typed_builtin())
	layer.set_meta("empty_enum_array_builtin", fields.empty_roles.get_typed_builtin())
	return layer
