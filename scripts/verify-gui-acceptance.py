#!/usr/bin/env python3
"""Run real native window captures; fail closed on missing GPU or input evidence."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import re
import struct
import subprocess
import time
import zlib

ROOT = Path(__file__).resolve().parents[1]


def png_size_and_colors(path):
    """Decode native_smoke's 8-bit RGBA PNG until enough distinct pixels exist."""
    data = path.read_bytes()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError("not a PNG")
    offset, compressed = 8, bytearray()
    while offset < len(data):
        size = struct.unpack_from(">I", data, offset)[0]
        kind, chunk = data[offset + 4:offset + 8], data[offset + 8:offset + 8 + size]
        if kind == b"IHDR":
            width, height, depth, color, _, _, interlace = struct.unpack(">IIBBBBB", chunk)
            if (depth, color, interlace) != (8, 6, 0):
                raise ValueError("expected an 8-bit RGBA non-interlaced capture")
        elif kind == b"IDAT":
            compressed.extend(chunk)
        offset += size + 12
    pixels = zlib.decompress(compressed)
    stride, previous, colors = width * 4, bytearray(width * 4), set()
    if len(pixels) != (stride + 1) * height:
        raise ValueError("incomplete PNG pixels")
    for row in range(height):
        start = row * (stride + 1)
        filtering, current = pixels[start], bytearray(pixels[start + 1:start + 1 + stride])
        for index in range(stride):
            left = current[index - 4] if index >= 4 else 0
            above = previous[index]
            corner = previous[index - 4] if index >= 4 else 0
            if filtering == 1:
                current[index] = (current[index] + left) & 255
            elif filtering == 2:
                current[index] = (current[index] + above) & 255
            elif filtering == 3:
                current[index] = (current[index] + (left + above) // 2) & 255
            elif filtering == 4:
                estimate = left + above - corner
                distances = [abs(estimate - value) for value in (left, above, corner)]
                predictor = (left, above, corner)[distances.index(min(distances))]
                current[index] = (current[index] + predictor) & 255
            elif filtering != 0:
                raise ValueError("invalid PNG filter")
        colors.update(bytes(current[x:x + 3]) for x in range(0, stride, 4))
        if len(colors) >= 32:
            break
        previous = current
    return [width, height], len(colors)


def validate_capture(name, text, screenshot, returncode):
    errors = []
    if returncode != 0:
        errors.append(f"native_smoke exit: {returncode}")
    adapter = next((line.removeprefix("GPU adapter: ") for line in text.splitlines()
                    if line.startswith("GPU adapter: ")), None)
    if not adapter:
        errors.append("missing GPU adapter evidence")
    kind = "unknown"
    if adapter:
        if re.search(r"llvmpipe|lavapipe|software|microsoft.*(?:basic|warp)|device_type: Cpu", adapter, re.I):
            kind = "software"
        elif "VirtualGpu" in adapter:
            kind = "virtual"
        elif "IntegratedGpu" in adapter or "DiscreteGpu" in adapter:
            kind = "hardware"
    states, surface_errors = {}, []
    for line in text.splitlines():
        for label in ("Capture initialized", "Workflow before", "Input before", "Input after", "Capture final", "Capture viewport", "Surface acquire errors (all frames)"):
            if line.startswith(label + ": "):
                try:
                    value = json.loads(line[len(label) + 2:])
                    if label.startswith("Surface"):
                        surface_errors.append(value)
                    else:
                        states[label] = value
                except json.JSONDecodeError:
                    errors.append(f"invalid JSON: {label}")
    viewport = states.get("Capture viewport", {})
    logical_size = viewport.get("logicalSize")
    ppp = viewport.get("pixelsPerPoint")
    if (not isinstance(logical_size, list) or len(logical_size) != 2
            or any(not isinstance(value, (int, float)) or isinstance(value, bool)
                   or not math.isfinite(value) or value <= 0 for value in logical_size + [ppp])):
        errors.append("missing or invalid logical viewport/DPI metadata")
        viewport = None
    final = states.get("Capture final", {})
    if not final or final.get("clicktrap") is not False:
        errors.append("missing capture state or active focus mask")
    if name != "home" and final.get("ready") is not True:
        errors.append("project/resources not ready")
    if name == "home" and final.get("level") is not None:
        errors.append("Home opened a project")
    if name in ("entities", "entity-form") and not (final.get("entityCount") or 0) > 0:
        errors.append("Entities layer not rendered")
    if name == "entity-form" and final.get("panel") != "Entities":
        errors.append("E shortcut did not open the Entities form")
    if name == "entity-drag":
        before = states.get("Workflow before", {})
        if (not before or before.get("gesture") is not None or before.get("stroke") is not False
                or final.get("gesture") != "EntityMove" or final.get("stroke") is not True
                or final.get("panel") != "Entity" or final.get("layerType") != "Entities"
                or not (before.get("entityCount") or 0) > 0
                or final.get("entityCount") != before.get("entityCount")
                or final.get("dirty") != before.get("dirty")):
            errors.append("held entity drag did not start or changed the document")
    if name == "world" and final.get("world") is not True:
        errors.append("world view not active")
    if name == "paint":
        before = states.get("Input before", {})
        if (final.get("layerType") != "IntGrid" or not before.get("intGridChecksum")
                or before.get("intGridChecksum") == final.get("intGridChecksum")
                or (final.get("intGridNonzero") or 0) <= (before.get("intGridNonzero") or 0)):
            errors.append("paint did not add IntGrid cells and change the checksum")
    expected_errors = {"Timeout", "Outdated", "Lost", "OutOfMemory", "Other"}
    if not surface_errors or any(set(value) != expected_errors or any(value.values()) for value in surface_errors):
        errors.append("missing counters or nonzero surface acquisition errors")
    if "Screenshot saved: " not in text or "screenshot=true" not in text:
        errors.append("missing verified screenshot completion")
    image_size = None
    try:
        image_size, colors = png_size_and_colors(screenshot)
        if viewport and any(abs(image_size[i] - logical_size[i] * ppp) > 1 for i in range(2)):
            errors.append("logical viewport/DPI disagrees with screenshot dimensions")
        if min(image_size) < 600 or colors < 32:
            errors.append("screenshot is too small or blank")
    except (OSError, ValueError, struct.error, zlib.error) as error:
        errors.append(f"invalid screenshot: {error}")
    metrics = [line for line in text.splitlines() if line.startswith(("Ready ", "Injected input ", "RSS KiB:", "Native smoke:"))]
    return {"errors": errors, "adapter": adapter, "adapter_kind": kind, "state": final,
            "states": states, "surface_errors": surface_errors, "image_size": image_size, "viewport": viewport, "metrics": metrics}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/release/examples" / ("native_smoke.exe" if os.name == "nt" else "native_smoke"))
    parser.add_argument("--output", "--output-dir", type=Path, default=ROOT / "dist/gui-acceptance")
    parser.add_argument("--fixtures", type=Path, default=ROOT / "examples/original")
    parser.add_argument("--source-commit", help="Source revision when running a copied harness outside its checkout")
    args = parser.parse_args()
    output, binary = args.output.resolve(), args.binary.resolve()
    output.mkdir(parents=True, exist_ok=True)
    scenarios = [
        ("home", None, {}),
        ("entities", "Entities.ldtk", {"VIEW": [408, 232, 1, False]}),
        ("entity-form", "Entities.ldtk", {"VIEW": [408, 232, 1, False], "KEY": "E"}),
        # Same PlayerStart and world delta as the recorded entity_move scenario.
        ("entity-drag", "Entities.ldtk", {"VIEW": [72, 360, 1, False], "ACTIONS": [
            {"frame": 0, "pointer": [870, 450], "button": "primary", "pressed": True},
            {"frame": 10, "pointer": [934, 418]}]}),
        ("world", "WorldMap_Free_layout.ldtk", {"VIEW": [256, -144, 0.5, True]}),
        ("paint", "AutoLayers_1_basic.ldtk", {"VIEW": [148, 104, 2, False], "KEY": "F1",
         "STROKE": [[x, 330] for x in range(750, 951, 8)]}),
    ]
    source_commit = args.source_commit
    if not source_commit:
        revision = subprocess.run(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
        source_commit = revision.stdout.strip() if revision.returncode == 0 else None
    run_url = None
    if os.environ.get("GITHUB_RUN_ID"):
        run_url = f'{os.environ["GITHUB_SERVER_URL"]}/{os.environ["GITHUB_REPOSITORY"]}/actions/runs/{os.environ["GITHUB_RUN_ID"]}'
    report = {"platform": platform.platform(), "machine": platform.machine(),
              "source_commit": source_commit,
              "run_url": run_url,
              "driver_environment": {key: os.environ[key] for key in ("DISPLAY", "WGPU_BACKEND", "VK_DRIVER_FILES", "LIBGL_ALWAYS_SOFTWARE") if key in os.environ},
              "scenarios": []}
    home_size = [1440, 900]
    for name, fixture, controls in scenarios:
        if name in ("entity-drag", "paint"):
            dx, dy = (home_size[0] - 1440) / 2, (home_size[1] - 900) / 2
            if name == "paint":
                controls["STROKE"] = [[x + dx, y + dy] for x, y in controls["STROKE"]]
            else:
                for action in controls["ACTIONS"]:
                    x, y = action["pointer"]
                    action["pointer"] = [x + dx, y + dy]
        screenshot = output / f"{name}.png"
        screenshot.unlink(missing_ok=True)
        config = output / f"{name}-config"
        config.mkdir(exist_ok=True)
        (config / "settings.json").write_text(json.dumps({"openLastProject": False, "recentProjects": [], "recentDirs": [], "appUiScale": 1, "editorUiScale": 1}))
        env = {key: value for key, value in os.environ.items() if not key.startswith("LDTK_SMOKE_")}
        env.update({"LDTK_CONFIG_DIR": str(config), "LDTK_SMOKE_OUTPUT": str(screenshot), "LDTK_SMOKE_FRAMES": "60", "LDTK_SMOKE_SETTLE_SECONDS": "1"})
        env.update({"LDTK_SMOKE_" + key: value if isinstance(value, str) else json.dumps(value) for key, value in controls.items()})
        command = [str(binary)]
        path = args.fixtures.resolve() / fixture if fixture else None
        before = hashlib.sha256(path.read_bytes()).hexdigest() if path else None
        if path:
            command.append(str(path))
        started = time.monotonic()
        try:
            process = subprocess.run(command, cwd=ROOT, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=90)
            text, returncode = process.stdout.decode("utf-8", errors="replace"), process.returncode
        except subprocess.TimeoutExpired as error:
            text, returncode = (error.stdout or b"").decode("utf-8", errors="replace"), "timeout"
        except OSError as error:
            text, returncode = str(error), "launch failed"
        (output / f"{name}.log").write_text(text, encoding="utf-8")
        result = validate_capture(name, text, screenshot, returncode)
        if name == "home" and result["viewport"]:
            home_size = result["viewport"]["logicalSize"]
        if path and hashlib.sha256(path.read_bytes()).hexdigest() != before:
            result["errors"].append("source fixture was modified")
        result.update({"name": name, "seconds": round(time.monotonic() - started, 3), "fixture": fixture, "fixture_sha256": before, "inputs": controls, "screenshot": screenshot.name, "log": f"{name}.log", "passed": not result["errors"]})
        report["scenarios"].append(result)
        report["passed"] = all(item["passed"] for item in report["scenarios"])
        (output / "results.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print(f"{name}: {'PASS' if result['passed'] else 'FAIL'}; adapter={result['adapter_kind']}; {', '.join(result['errors'])}", flush=True)
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
