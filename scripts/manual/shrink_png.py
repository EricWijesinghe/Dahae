# Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
# SPDX-License-Identifier: LicenseRef-Proprietary
"""Makes the manual screenshots print-ready and small.

1. Trims empty page below the content (a single card rendered on a tall view).
2. Re-encodes to a 256-colour palette when a PNG is over the size limit.
Usage: python -I shrink_png.py <dir> [max_kb]
"""
import sys
from pathlib import Path

from PIL import Image, ImageChops

MARGIN = 48  # px of background kept under the content


def trim_bottom(img: Image.Image) -> Image.Image:
    rgb = img.convert("RGB")
    bg = Image.new("RGB", rgb.size, rgb.getpixel((rgb.width - 1, rgb.height - 1)))
    box = ImageChops.difference(rgb, bg).point(lambda v: 255 if v > 6 else 0).getbbox()
    if not box or box[3] + MARGIN >= rgb.height:
        return img
    return img.crop((0, 0, img.width, box[3] + MARGIN))


def shrink(path: Path, max_kb: int) -> None:
    img = Image.open(path)
    img.load()
    out = trim_bottom(img)
    if out is not img:
        out.save(path, optimize=True)
    if path.stat().st_size > max_kb * 1024:
        out.convert("RGB").quantize(colors=256, method=Image.Quantize.MEDIANCUT, dither=Image.Dither.NONE).save(
            path, optimize=True
        )
    if path.stat().st_size > max_kb * 1024:
        print(f"still over {max_kb} KB: {path} ({path.stat().st_size // 1024} KB)")


def main() -> None:
    root = Path(sys.argv[1])
    max_kb = int(sys.argv[2]) if len(sys.argv) > 2 else 400
    for p in sorted(root.rglob("*.png")):
        shrink(p, max_kb)


if __name__ == "__main__":
    main()
