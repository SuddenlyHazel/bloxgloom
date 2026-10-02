"""Import terrain and companion maps: uv run --with pillow this_file.py BLOCK_DIR."""

import argparse
from pathlib import Path

from PIL import Image, ImageChops


def tinted_grass(image):
    rgba = image.convert("RGBA")
    rgb = ImageChops.multiply(rgba.convert("RGB"), Image.new("RGB", rgba.size, (145, 189, 89)))
    rgb.putalpha(rgba.getchannel("A"))
    return rgb


def resized_data(image):
    # RGBA image resizing premultiplies RGB by alpha. Height is independent
    # data here, so filter every channel separately instead.
    return Image.merge("RGBA", tuple(
        channel.resize((128, 128), Image.Resampling.BOX)
        for channel in image.convert("RGBA").split()
    ))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("block_dir", type=Path)
    parser.add_argument("--output", type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args()
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
    images["grass_top"] = tinted_grass(images["grass_top"])
    images["grass_side"] = Image.alpha_composite(images["grass_side"], tinted_grass(overlay))
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
