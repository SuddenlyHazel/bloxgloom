"""Import Rock030: uv run --with pillow this_file.py ROCK030_1K_PNG_DIR."""

import argparse
from pathlib import Path

from PIL import Image, ImageOps


SIZE = (128, 128)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("--output", type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args()
    prefix = "Rock030_1K-PNG"
    # Decode and pack all maps before replacing the existing stone material.
    color = Image.open(args.source / f"{prefix}_Color.png").convert("RGB")
    normal = Image.open(args.source / f"{prefix}_NormalDX.png").convert("RGB")
    roughness = Image.open(args.source / f"{prefix}_Roughness.png").convert("L")
    displacement = Image.open(args.source / f"{prefix}_Displacement.png")
    if {color.size, normal.size, roughness.size, displacement.size} != {(1024, 1024)}:
        raise ValueError("Expected four matching 1024x1024 Rock030 source maps")
    if displacement.mode not in ("I;16", "I;16L", "I;16B"):
        raise ValueError("Expected the original 16-bit displacement map")

    # Height is linear independent data. Filter in float before quantizing the
    # original 0..65535 range to 0..255; converting I;16 straight to L clips it.
    height = displacement.convert("F").resize(SIZE, Image.Resampling.BOX)
    height = height.point(lambda value: value / 257.0).convert("L")
    normals = tuple(channel.resize(SIZE, Image.Resampling.BOX) for channel in normal.split())
    normal_height = Image.merge("RGBA", (*normals, height))

    # The renderer's oldPBR data is smoothness R, metallic G. Rock is dielectric.
    smoothness = ImageOps.invert(roughness.resize(SIZE, Image.Resampling.BOX))
    black = Image.new("L", SIZE, 0)
    specular = Image.merge("RGBA", (smoothness, black, black, Image.new("L", SIZE, 255)))
    outputs = {
        "stone.png": color.resize(SIZE, Image.Resampling.LANCZOS),
        "stone_n.png": normal_height,
        "stone_s.png": specular,
    }
    args.output.mkdir(parents=True, exist_ok=True)
    for name, image in outputs.items():
        image.save(args.output / name, optimize=True)


if __name__ == "__main__":
    main()
