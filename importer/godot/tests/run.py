#!/usr/bin/env python3
"""Exercise the copied addon through Godot's actual editor import pipeline."""
import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[1]
LOGS = ROOT.parents[1] / ".tools/godot-importer"


def run(godot, project, label, *args):
    result = subprocess.run(
        [godot, "--headless", "--path", str(project), "--log-file", str(project / (label + ".log")), *args],
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, timeout=90,
    )
    LOGS.mkdir(parents=True, exist_ok=True)
    (LOGS / (label + ".log")).write_text(result.stdout)
    if result.returncode or "SCRIPT ERROR:" in result.stdout or "ERROR:" in result.stdout or "invalid UID:" in result.stdout:
        raise RuntimeError(f"{label} failed (exit {result.returncode}):\n{result.stdout[-6500:]}")
    print(f"PASS: {label}")


def verify(godot, project, label, tile_count=2, health=25, multi_world=False, override=False):
    expected = project / "expected.json"
    expected.write_text(json.dumps({"tile_count": tile_count, "health": health, "multi_world": multi_world, "override": override}))
    run(godot, project, label, "--script", "res://tests/check_import.gd", "--", str(expected))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--godot", default="godot", help="Godot executable (default: godot on PATH)")
    args = parser.parse_args()
    version = subprocess.check_output([args.godot, "--version"], text=True).strip()
    print(version)
    LOGS.mkdir(parents=True, exist_ok=True)
    (LOGS / "results.json").unlink(missing_ok=True)
    completed = []
    with tempfile.TemporaryDirectory(prefix="LevelEditor Godot importer ") as temp:
        for case in ("1.5.4", "1.5.3", "external", "multi-world", "multi-world-override"):
            project = Path(temp) / case
            project.mkdir()
            shutil.copy2(ROOT / "project.godot", project / "project.godot")
            shutil.copy2(ROOT / "demo.tscn", project / "demo.tscn")
            with (project / "project.godot").open("a") as settings:
                settings.write('\n[importer_defaults]\nldtk.import={"entities_post_import": "res://tests/field_types_post_import.gd"}\n')
            shutil.copytree(ROOT / "tests", project / "tests", ignore=shutil.ignore_patterns("__pycache__", "*.uid", "*.import"))
            if (ROOT / "addons").exists():
                shutil.copytree(ROOT / "addons", project / "addons")
            source = project / "tests/maps/smoke.ldtk"
            data = json.loads(source.read_text())
            if case == "1.5.3":
                data["jsonVersion"] = "1.5.3"
            elif case == "external":
                external = source.parent / "external/Smoke.ldtkl"
                external.parent.mkdir()
                external.write_text(json.dumps(data["levels"][0]))
                data["externalLevels"] = True
                data["levels"][0]["layerInstances"] = None
                data["levels"][0]["externalRelPath"] = "external/Smoke.ldtkl"
            elif case.startswith("multi-world"):
                second = copy.deepcopy(data["levels"][0])
                second.update(identifier="Other", uid=40, iid="00000000-0000-0000-0000-000000000040")
                for index, layer in enumerate(second["layerInstances"], 41):
                    layer.update(iid=f"00000000-0000-0000-0000-{index:012d}", levelId=40)
                    for entity in layer["entityInstances"]:
                        entity["iid"] = "00000000-0000-0000-0000-000000000043"
                if case == "multi-world-override":
                    tileset = copy.deepcopy(data["defs"]["tilesets"][0])
                    tileset.update(uid=60, identifier="SmallTiles", tileGridSize=8, __cWid=4, __cHei=2)
                    data["defs"]["tilesets"].append(tileset)
                    ground = second["layerInstances"][1]
                    ground.update(overrideTilesetUid=60, __tilesetDefUid=60)
                    ground["gridTiles"][1]["src"] = [8, 0]
                data["worlds"] = [
                    {"iid": "00000000-0000-0000-0000-000000000016", "identifier": "First", "worldLayout": "Free", "worldGridWidth": 16, "worldGridHeight": 16, "levels": data["levels"]},
                    {"iid": "00000000-0000-0000-0000-000000000017", "identifier": "Second", "worldLayout": "Free", "worldGridWidth": 16, "worldGridHeight": 16, "levels": [second]},
                ]
                data.update(worldLayout=None, levels=[], nextUid=61)
            source.write_text(json.dumps(data))
            inputs = [source, source.with_name("tiles.png"), *source.parent.rglob("*.ldtkl")]
            before = {p: hashlib.sha256(p.read_bytes()).hexdigest() for p in inputs}
            run(args.godot, project, case + "-import", "--editor", "--import")
            verify(args.godot, project, case + "-scene", multi_world=case.startswith("multi-world"), override=case == "multi-world-override")
            assert all(hashlib.sha256(p.read_bytes()).hexdigest() == digest for p, digest in before.items()), "Importer changed source map/image"
            completed.append(case)
            if case == "1.5.4":
                run(args.godot, project, "demo-start", "--quit-after", "3")
                data["levels"][0]["layerInstances"][1]["gridTiles"].pop()
                health = data["levels"][0]["layerInstances"][0]["entityInstances"][0]["fieldInstances"][0]
                health.update(__value=50, realEditorValues=[{"id": "V_Int", "params": [50]}])
                modified = source.stat().st_mtime + 2
                source.write_text(json.dumps(data))
                os.utime(source, (modified, modified))
                run(args.godot, project, "reimport", "--editor", "--import")
                verify(args.godot, project, "reimport-scene", tile_count=1, health=50)
                completed.append("reimport")
    (LOGS / "results.json").write_text(json.dumps({
        "success": True,
        "godot_version": version,
        "godot_executable": str(Path(shutil.which(args.godot) or args.godot).resolve()),
        "upstream_commit": "0ecab8d1cc6531801b487157848059c3b8971aa0",
        "scenarios": completed,
        "checks": ["PackedScene", "TileMapLayer", "atlas coordinates", "tile flip", "entity position", "typed nullable arrays", "enum Array[String] in actual Post-Import", "world order", "multi-world tileset override", "source files unchanged", "no errors or invalid UIDs"],
    }, indent=2) + "\n")
    print("All Godot importer checks passed.")


if __name__ == "__main__":
    main()
