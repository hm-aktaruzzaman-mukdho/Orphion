#!/usr/bin/env python3
# Regenerates assets/ (needs Pillow). Usage:
#   python3 packaging/make-icons.py <MaterialSymbolsRounded_Filled-Regular.ttf> assets
# The font ships with the egui_material_icons crate (~/.cargo/registry/src/*/egui_material_icons-*/).
"""Renders the app icon: the Material Symbols "earbuds" glyph on a rounded square."""
import sys
from PIL import Image, ImageDraw, ImageFont

font_path, out_dir = sys.argv[1], sys.argv[2]
BG = (11, 87, 208, 255)   # M3 primary (light scheme) #0B57D0
FG = (255, 255, 255, 255)
GLYPH = ""          # earbuds

def render(size: int) -> Image.Image:
    scale = 4  # supersample for smooth edges
    s = size * scale
    img = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.rounded_rectangle([0, 0, s - 1, s - 1], radius=int(s * 0.28), fill=BG)
    font = ImageFont.truetype(font_path, int(s * 0.66))
    box = d.textbbox((0, 0), GLYPH, font=font)
    w, h = box[2] - box[0], box[3] - box[1]
    d.text(((s - w) / 2 - box[0], (s - h) / 2 - box[1]), GLYPH, font=font, fill=FG)
    return img.resize((size, size), Image.LANCZOS)

big = render(256)
big.save(f"{out_dir}/orphion.png")
big.save(f"{out_dir}/orphion.ico", sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
# Raw RGBA for the tray / window icon (no image decoder needed at runtime).
for size in (32, 64):
    open(f"{out_dir}/icon-{size}.rgba", "wb").write(render(size).tobytes())
print("ok")
