#!/usr/bin/env python3
"""Matched renderer acceptance captures, not a performance benchmark.

Use one immutable executable for every toggle. Adapter selection is inherited;
set your backend/adapter environment before running. No screenshots are altered.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("executable", type=Path)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--motion", action="store_true", help="Include the 28-frame GLB/character motion pairs")
    parser.add_argument("--sandbox", action="store_true", help="Include factory/noon and neon/night regression views")
    args = parser.parse_args()
    executable = args.executable.resolve(strict=True)
    output = args.directory.resolve()
    output.mkdir(parents=True, exist_ok=True)
    common = {
        "BLOXGLOOM_SUN_SHADOWS": "high",
        "BLOXGLOOM_SUN_SOFTNESS": "1",
        "BLOXGLOOM_CONTACT_OCCLUSION": "1",
        "BLOXGLOOM_AO": "0.75",
        "BLOXGLOOM_AO_RADIUS": "1.5",
        "BLOXGLOOM_TAA": "0",
    }
    cases = [
        ("depth-ao-off", "outdoor-depth-preview", {"BLOXGLOOM_AO": "0"}, []),
        ("depth-ao-on", "outdoor-depth-preview", {}, []),
        ("outdoor-shadow-fixed", "outdoor-creature-preview", {"BLOXGLOOM_SUN_SOFTNESS": "0"}, []),
        ("outdoor-shadow-soft", "outdoor-creature-preview", {}, []),
    ]
    if args.motion:
        cases += [
            ("motion-aa-off", "outdoor-creature-motion-preview", {}, []),
            ("motion-aa-on", "outdoor-creature-motion-preview", {"BLOXGLOOM_TAA": "1"}, []),
        ]
    if args.sandbox:
        cases += [
            ("factory", "sandbox-preview", {}, ["factory", "noon", "hero", "idle"]),
            ("neon", "sandbox-preview", {}, ["neon", "night", "hero", "idle"]),
        ]
    manifest = {
        "executable": str(executable),
        "sha256": hashlib.file_digest(executable.open("rb"), "sha256").hexdigest(),
        "purpose": "Matched visual correctness; no hardware performance claims",
        "inherited_adapter_environment": {key: os.environ.get(key) for key in (
            "WGPU_BACKEND", "WGPU_ADAPTER_NAME", "VK_ICD_FILENAMES"
        )},
        "cases": [],
    }
    for name, command, changes, tail in cases:
        settings = common | changes
        invocation = [str(executable), command, str(output / name), *tail]
        record = {"name": name, "command": invocation, "settings": settings, "status": "running"}
        manifest["cases"].append(record)
        manifest_path = output / "manifest.json"
        manifest_path.write_text(json.dumps(manifest, indent=2) + "\n")
        print(name, flush=True)
        with (output / f"{name}.log").open("w") as log:
            result = subprocess.run(invocation, env=os.environ | settings, stdout=log, stderr=subprocess.STDOUT)
        record["exit_code"] = result.returncode
        record["status"] = "passed" if result.returncode == 0 else "failed"
        manifest_path.write_text(json.dumps(manifest, indent=2) + "\n")
        if result.returncode:
            raise SystemExit(f"{name} failed; inspect {output / (name + '.log')}")


if __name__ == "__main__":
    main()
