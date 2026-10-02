"""Import NAPP textures: uv run --with pillow this_file.py BLOCK_DIR [--natural-only]."""

import argparse
from pathlib import Path

from PIL import Image, ImageChops


def tinted(image, color=(145, 189, 89)):
    rgba = image.convert("RGBA")
    rgb = ImageChops.multiply(rgba.convert("RGB"), Image.new("RGB", rgba.size, color))
    rgb.putalpha(rgba.getchannel("A"))
    return rgb


def resized_data(image):
    # RGBA image resizing premultiplies RGB by alpha. Height is independent
    # data here, so filter every channel separately instead.
    return Image.merge("RGBA", tuple(
        channel.resize((128, 128), Image.Resampling.BOX)
        for channel in image.convert("RGBA").split()
    ))


def import_natural(block_dir, output):
    sources = {
        output / "wood_side.png": block_dir / "oak_log.png",
        output / "wood_top.png": block_dir / "oak_log_top.png",
        output.parent / "foliage/leaves.png": block_dir / "oak_leaves.png",
        output.parent / "items/stick.png": block_dir.parent / "item/stick.png",
    }
    # Decode all albedo and data maps before touching the existing assets.
    images = {target: Image.open(source).convert("RGBA")
              for target, source in sources.items()}
    leaves = output.parent / "foliage/leaves.png"
    images[leaves] = tinted(images[leaves], (119, 171, 47))
    companions = {}
    for target, source in sources.items():
        if target == leaves:  # This pack has no oak-leaf companion maps.
            continue
        for suffix in ("n", "s"):
            companions[target.with_name(f"{target.stem}_{suffix}.png")] = Image.open(
                source.with_name(f"{source.stem}_{suffix}.png")
            ).convert("RGBA")
    for target, image in images.items():
        target.parent.mkdir(parents=True, exist_ok=True)
        if target.parent == output:
            image = image.convert("RGB")
        image.resize((128, 128), Image.Resampling.LANCZOS).save(target, optimize=True)
    for target, image in companions.items():
        resized_data(image).save(target, optimize=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("block_dir", type=Path)
    parser.add_argument("--output", type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument("--natural-only", action="store_true",
                        help="Import oak logs, tinted oak leaves, and the stick into sibling asset folders")
    args = parser.parse_args()
    if args.natural_only:
        import_natural(args.block_dir, args.output)
        return
    sources = {
        "grass_top": "grass_block_top",
        "grass_side": "grass_block_side",
        "dirt": "dirt",
        "stone": "stone",
        "sand": "sand",
        "snow": "snow",
        "moss": "moss_block",
        "gravel": "gravel",
        "glowstone": "glowstone",
    }
    # Decode every input before replacing any output; a partial pack must fail cleanly.
    images = {name: Image.open(args.block_dir / f"{source}.png").convert("RGBA")
              for name, source in sources.items()}
    overlay = Image.open(args.block_dir / "grass_block_side_overlay.png").convert("RGBA")
    images["grass_top"] = tinted(images["grass_top"])
    images["grass_side"] = Image.alpha_composite(images["grass_side"], tinted(overlay))
    companions = {}
    for name, source in sources.items():
        for suffix in ("n", "s"):
            companions[f"{name}_{suffix}"] = Image.open(
                args.block_dir / f"{source}_{suffix}.png"
            ).convert("RGBA")
    args.output.mkdir(parents=True, exist_ok=True)
    for name, image in images.items():
        image.convert("RGB").resize((128, 128), Image.Resampling.LANCZOS).save(
            args.output / f"{name}.png", optimize=True
        )
    # Data maps use box filtering, not color/gamma processing or Lanczos ringing.
    # Shader decoding normalizes interpolated RGB normals; alpha retains height.
    for name, image in companions.items():
        resized_data(image).save(
            args.output / f"{name}.png", optimize=True
        )


if __name__ == "__main__":
    main()
