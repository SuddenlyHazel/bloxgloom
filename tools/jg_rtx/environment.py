#!/usr/bin/env python3
"""Import unchanged JG RTX Java celestial artwork and record exact provenance."""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys


def main(source: Path) -> None:
    root = Path(__file__).resolve().parents[2]
    destination = root / "assets/textures/environment"
    destination.mkdir(parents=True, exist_ok=True)
    records = []
    for filename in ("sun.png", "moon_phases.png"):
        relative = Path("java/pack/assets/minecraft/textures/environment") / filename
        original = source / relative
        output = destination / filename
        shutil.copyfile(original, output)
        records.append({
            "source": relative.as_posix(),
            "destination": output.relative_to(root).as_posix(),
            "sha256": hashlib.sha256(original.read_bytes()).hexdigest(),
            "processing": "unchanged source bytes; runtime sRGB texture decoding",
        })
    manifest = {
        "source": "https://github.com/jasonjgardner/jg-rtx",
        "source_revision": subprocess.check_output(["git", "-C", str(source), "rev-parse", "HEAD"], text=True).strip(),
        "license": "CC-BY-SA-4.0; retained in assets/jg-rtx/LICENSE",
        "attribution": "Jason J. Gardner and upstream contributors; assets/jg-rtx/CREDITS.md",
        "moon_atlas": {"columns": 4, "rows": 2, "phase_indices": "server day modulo 8; row-major"},
        "textures": records,
    }
    (root / "assets/jg-rtx/environment.json").write_text(json.dumps(manifest, indent=2) + "\n")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("usage: environment.py /path/to/jg-rtx")
    main(Path(sys.argv[1]).resolve())
