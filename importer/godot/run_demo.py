#!/usr/bin/env python3
"""Open the bundled LDtk platformer in an independent Godot demo project."""

import json
from pathlib import Path
import shutil
import subprocess
import sys


IMPORTER = Path(__file__).resolve().parent
REPO = IMPORTER.parents[1]
PROJECT = REPO / ".tools/godot-platformer-demo"


def write_if_changed(path, contents):
    data = contents.encode("utf-8")
    if not path.exists() or path.read_bytes() != data:
        path.write_bytes(data)


def main():
    godot = shutil.which("godot")
    if godot is None:
        sys.exit("Godot command not found. Add your installed Godot executable to PATH.")

    samples = REPO / "examples/original"
    maps = PROJECT / "maps"
    (maps / "atlas").mkdir(parents=True, exist_ok=True)
    shutil.copytree(IMPORTER / "addons/ldtk-importer", PROJECT / "addons/ldtk-importer", dirs_exist_ok=True)

    document = json.loads((samples / "Typical_2D_platformer_example.ldtk").read_text(encoding="utf-8"))
    for tileset in document["defs"]["tilesets"]:
        if tileset["embedAtlas"] == "LdtkIcons":
            # Godot imports textures by path. Use the same atlas as LevelEditor.
            tileset["relPath"] = "atlas/finalbossblues-icons_full_16.png"
            tileset["embedAtlas"] = None
            source = REPO / "assets/embedAtlas" / Path(tileset["relPath"]).name
        else:
            source = samples / tileset["relPath"]
        shutil.copy2(source, maps / tileset["relPath"])
    write_if_changed(maps / "platformer.ldtk", json.dumps(document, ensure_ascii=False))

    write_if_changed(PROJECT / "project.godot", '''config_version=5

[application]
config/name="LevelEditor - LDtk Platformer Preview"
run/main_scene="res://demo.tscn"
config/features=PackedStringArray("4.3", "GL Compatibility")

[display]
window/size/viewport_width=960
window/size/viewport_height=480
window/stretch/mode="canvas_items"

[editor_plugins]
enabled=PackedStringArray("res://addons/ldtk-importer/plugin.cfg")

[importer_defaults]
ldtk.import={"pack_levels": false, "use_entity_placeholders": true}

[rendering]
renderer/rendering_method="gl_compatibility"
textures/canvas_textures/default_texture_filter=0
environment/defaults/default_clear_color=Color(0.19, 0.18, 0.25, 1)
''')
    write_if_changed(PROJECT / "demo.tscn", '''[gd_scene load_steps=2 format=3]

[ext_resource type="PackedScene" path="res://maps/platformer.ldtk" id="1_map"]

[node name="Demo" type="Node2D"]

[node name="Map" parent="." instance=ExtResource("1_map")]

[node name="Camera2D" type="Camera2D" parent="."]
position = Vector2(424, 168)
zoom = Vector2(1.08, 1.08)
''')
    return subprocess.call([godot, "--editor", "--path", str(PROJECT), *sys.argv[1:]])


if __name__ == "__main__":
    sys.exit(main())
