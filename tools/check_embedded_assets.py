#!/usr/bin/env python3
"""Check that literal Rust embedded assets exist in a fresh Git checkout."""
import re
import subprocess
from pathlib import Path


def main():
    root = Path(__file__).resolve().parents[1]
    tracked = set(
        subprocess.check_output(["git", "ls-files", "-z"], cwd=root)
        .decode("utf-8")
        .split("\0")
    )
    errors = []
    count = 0
    for name in sorted(tracked):
        source = root / name
        if source.suffix != ".rs":
            continue
        for match in re.finditer(
            r'include_(?:bytes|str)!\s*\(\s*"([^"\n]+)"', source.read_text()
        ):
            count += 1
            target = (source.parent / match[1]).resolve()
            try:
                relative = target.relative_to(root).as_posix()
            except ValueError:
                errors.append(f"{name}: embedded path leaves the repo: {match[1]}")
                continue
            if relative not in tracked or not target.is_file():
                errors.append(f"{name}: missing/untracked embedded asset: {relative}")
    for error in errors:
        print(error)
    print(f"Checked {count} literal embedded assets; {len(errors)} problems.")
    return bool(errors)


if __name__ == "__main__":
    raise SystemExit(main())
