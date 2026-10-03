#!/usr/bin/env python3
"""Check clearly exposed sky in matched outdoor-motion-preview sequences.

Requires Pillow and NumPy. This is a narrow, fixture-specific ghosting check, not
an image-quality score: moving interiors and non-sky backgrounds still need review.
Run both sequences with the same executable, adapter, and lighting settings.
"""
import argparse
import json
from pathlib import Path

import numpy as np
from PIL import Image


def read(directory, frame):
    return np.asarray(Image.open(directory / f"{frame:02}.png").convert("RGB"), dtype=np.int16)


def sky(rgb):
    return ((rgb[:, :, 2] > 150) & (rgb[:, :, 1] > 140)
            & (rgb[:, :, 0] > 100) & (rgb[:, :, 2] > rgb[:, :, 0] + 5))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("off", type=Path)
    parser.add_argument("taa", type=Path)
    args = parser.parse_args()
    # Check the full bounded sequence exists, including the resize/cut frames.
    for frame in range(28):
        off, taa = read(args.off, frame), read(args.taa, frame)
        expected = (400, 640, 3) if frame < 23 else (500, 800, 3)
        if off.shape != expected or taa.shape != expected:
            raise ValueError(f"frame {frame}: mismatched capture dimensions")
    rows = []
    for frame in range(8, 20):
        current = read(args.off, frame)
        previous = read(args.off, frame - 1)
        temporal = read(args.taa, frame)
        mask = sky(current)
        interior = mask.copy()
        for y in (-1, 0, 1):
            for x in (-1, 0, 1):
                interior &= np.roll(np.roll(mask, y, 0), x, 1)
        interior[[0, -1], :] = False
        interior[:, [0, -1]] = False
        # Exclude the one-pixel silhouette neighborhood, where current jitter can
        # legitimately change coverage. Count newly uncovered dark geometry only.
        exposed = interior & ~sky(previous) & (previous.max(2) < 140)
        rows.append({"frame": frame, "exposed_sky_pixels": int(exposed.sum()),
                     "lingering_dark_pixels": int((exposed & ~sky(temporal)).sum())})
    total = sum(row["exposed_sky_pixels"] for row in rows)
    lingering = sum(row["lingering_dark_pixels"] for row in rows)
    print(json.dumps({"frames": rows, "exposed_sky_pixels": total,
                      "lingering_dark_pixels": lingering,
                      "scope": "sky disocclusions outside one-pixel edge neighborhood"}, indent=2))
    if total < 100:
        raise SystemExit("insufficient exposed sky: check fixture and capture settings")
    if lingering:
        raise SystemExit("dark history remains in clearly exposed sky; inspect paired frames")


if __name__ == "__main__":
    main()
