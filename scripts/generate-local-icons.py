"""Reproduce the original local-fork icons; requires Pillow. GPL-3.0-only."""
from pathlib import Path
from PIL import Image, ImageDraw

root = Path(__file__).resolve().parents[1]
icon = Image.new("RGBA", (256, 256), (20, 28, 43, 255))
draw = ImageDraw.Draw(icon)
draw.rounded_rectangle((76, 27, 180, 229), radius=32, fill=(37, 52, 74), outline=(111, 217, 189), width=7)
draw.ellipse((113, 48, 143, 78), fill=(111, 217, 189))
draw.ellipse((94, 93, 162, 161), outline=(226, 236, 249), width=6)
draw.ellipse((118, 117, 138, 137), fill=(226, 236, 249))
draw.line([(99, 190), (114, 179), (99, 168)], fill=(111, 217, 189), width=5)
draw.line([(130, 194), (155, 194)], fill=(111, 217, 189), width=5)
icon.save(root / "public/app-logo.png")
icon.resize((32, 32), Image.Resampling.LANCZOS).save(root / "public/favicon.png")
for path in (root / "src-tauri/icons").glob("*.png"):
    with Image.open(path) as current:
        dimensions = current.size
    icon.resize(dimensions, Image.Resampling.LANCZOS).save(path)
icon.save(root / "src-tauri/icons/icon.ico", sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
