#!/usr/bin/env python3
"""Measure real native canvas renders during repeatable large-map input."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import statistics
import subprocess


ROOT = Path(__file__).resolve().parents[1]


def scenarios():
    zoom = [
        {"frame": frame, "pointer": [870, 450], "wheel": 100 if frame % 12 < 6 else -100}
        for frame in range(181)
    ]
    drag = [{"frame": frame, "pointer": [650 + min(frame % 80, 80 - frame % 80) * 10, 445]}
            for frame in range(181)]
    drag[0].update(button="middle", pressed=True)
    drag[-1].update(button="middle", pressed=False)
    paint = [[x, y] for row, y in enumerate(range(320, 480, 16))
             for x in (range(600, 888, 16) if row % 2 == 0 else range(872, 584, -16))]
    return {
        "zoom": {"VIEW": [1856, 864, 0.45, False], "ACTIONS": zoom},
        "drag": {"VIEW": [1856, 864, 0.45, False], "ACTIONS": drag},
        "paint": {"VIEW": [512, 512, 1, False], "KEY": "F4", "STROKE": paint},
    }


def run(binary, project, output, scenario, controls, vsync):
    output.mkdir(parents=True, exist_ok=True)
    screenshot = output / "frame.png"
    screenshot.unlink(missing_ok=True)
    config = output / "config"
    config.mkdir(exist_ok=True)
    (config / "settings.json").write_text(json.dumps({"autoWorldModeSwitch": "Never"}))
    env = {key: value for key, value in os.environ.items() if not key.startswith("LDTK_SMOKE_")}
    env.update({"LDTK_CONFIG_DIR": str(config), "LDTK_SMOKE_OUTPUT": str(screenshot),
                "LDTK_SMOKE_BENCH": "1"})
    if not vsync:
        env["LDTK_SMOKE_NO_VSYNC"] = "1"
    env.update({f"LDTK_SMOKE_{key}": value if isinstance(value, str) else json.dumps(value)
                for key, value in controls.items()})
    with (output / "native.log").open("w") as log:
        result = subprocess.run([str(binary), str(project)], cwd=ROOT, env=env,
                                stdout=log, stderr=subprocess.STDOUT, timeout=90)
    log = (output / "native.log").read_text()
    if result.returncode or not screenshot.is_file():
        raise RuntimeError(f"{scenario} did not finish: {log[-3000:]}")

    def state(prefix):
        return json.loads(re.search(rf"^{re.escape(prefix)} (.+)$", log, re.M)[1])

    begin = state("Input before:" if scenario == "paint" else "Workflow before:")
    end = state("Input after:")
    if scenario == "paint":
        assert begin["layer"] == end["layer"] == "Walls" and not end["stroke"], (begin, end)
        assert begin["intGridChecksum"] != end["intGridChecksum"], (begin, end)
    elif scenario == "zoom":
        assert begin["view"]["zoom"] != end["view"]["zoom"], (begin, end)
    else:
        assert begin["view"]["center"] != end["view"]["center"], (begin, end)
    assert begin["level"] == end["level"] == "Level_0" and not end["world"], (begin, end)
    assert not any(state("Surface acquire errors (all frames):").values()), log[-1000:]
    rendered, elapsed, fps = re.search(r"Input rendering: (\d+) frames in ([\d.]+)s; ([\d.]+) FPS", log).groups()
    assert int(rendered) == (len(controls["STROKE"]) + 1 if scenario == "paint" else len(controls["ACTIONS"])), log[-1000:]

    def timing(label):
        match = re.search(rf"{label}: \d+ samples; median ([\d.]+)ms; p95 ([\d.]+)ms; max ([\d.]+)ms", log)
        return dict(zip(("median", "p95", "max"), map(float, match.groups())))

    return {"fps": float(fps), "frames": int(rendered), "seconds": float(elapsed),
            "interval_ms": timing("Injected input update-start wall interval"),
            "app_update_ms": timing("Injected input app.update CPU"),
            "viewport": state("Capture viewport:"), "begin": begin, "end": end}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/release/examples/native_smoke")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--repeat", type=int, default=3)
    parser.add_argument("--scenario", choices=scenarios())
    parser.add_argument("--vsync", action="store_true", help="use the editor's normal presentation mode")
    args = parser.parse_args()
    if args.repeat < 1:
        parser.error("--repeat must be positive")
    project = ROOT / ".reference/ldtk/tests/largeLevels/world1.ldtk"
    if not project.is_file():
        parser.error(f"missing benchmark map: {project}. See README for the reference checkout.")
    source_hash = hashlib.sha256(project.read_bytes()).hexdigest()
    suite = {name: controls for name, controls in scenarios().items()
             if args.scenario in (None, name)}
    results = {name: [] for name in suite}
    for attempt in range(args.repeat):
        for name, controls in suite.items():
            sample = run(args.binary.resolve(), project, args.output.resolve() / f"{name}-{attempt + 1}",
                         name, controls, args.vsync)
            results[name].append(sample)
            print(f"{name} {attempt + 1}: {sample['fps']:.1f} FPS, interval p95 {sample['interval_ms']['p95']:.2f} ms, "
                  f"CPU p95 {sample['app_update_ms']['p95']:.2f} ms", flush=True)
    assert hashlib.sha256(project.read_bytes()).hexdigest() == source_hash
    summary = {name: {"fps": statistics.median(s["fps"] for s in samples),
                      "interval_p95_ms": statistics.median(s["interval_ms"]["p95"] for s in samples),
                      "cpu_p95_ms": statistics.median(s["app_update_ms"]["p95"] for s in samples)}
               for name, samples in results.items()}
    report = {"fixture_sha256": source_hash, "vsync": args.vsync, "summary": summary, "runs": results}
    (args.output / "results.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
