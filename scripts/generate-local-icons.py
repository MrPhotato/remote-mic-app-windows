"""Draw the original SayAll remote/voice icon. Requires Pillow; GPL-3.0-only.

The 1024px master is drawn at native resolution (never enlarged from a tiny
icon). All application, installer and tray assets share this master. Optional
--preview writes a local review board with real-size light/dark samples.
"""
import argparse
import json
import math
from pathlib import Path
from PIL import Image, ImageDraw, ImageFilter, ImageFont

ROOT = Path(__file__).resolve().parents[1]
SIZE = 1024
ICO_SIZES = (16, 24, 32, 48, 64, 128, 256)
PNG_SIZES = {
    "icon-source.png": 1024, "icon.png": 512,
    "32x32.png": 32, "64x64.png": 64,
    "128x128.png": 128, "128x128@2x.png": 256,
    "Square30x30Logo.png": 30, "Square44x44Logo.png": 44,
    "Square71x71Logo.png": 71, "Square89x89Logo.png": 89,
    "Square107x107Logo.png": 107, "Square142x142Logo.png": 142,
    "Square150x150Logo.png": 150, "Square284x284Logo.png": 284,
    "Square310x310Logo.png": 310, "StoreLogo.png": 50,
}


def gradient(top, bottom):
    image = Image.new("RGBA", (SIZE, SIZE))
    draw = ImageDraw.Draw(image)
    for y in range(SIZE):
        t = y / (SIZE - 1)
        color = tuple(round(a + (b - a) * t) for a, b in zip(top, bottom))
        draw.line((0, y, SIZE, y), fill=(*color, 255))
    return image


def master():
    tile = gradient((43, 43, 80), (12, 22, 38))
    glow = Image.new("RGBA", tile.size)
    d = ImageDraw.Draw(glow)
    d.ellipse((40, 15, 610, 470), fill=(140, 119, 255, 88))
    d.ellipse((585, 630, 1030, 1090), fill=(56, 210, 172, 75))
    tile = Image.alpha_composite(tile, glow.filter(ImageFilter.GaussianBlur(105)))
    mask = Image.new("L", tile.size)
    ImageDraw.Draw(mask).rounded_rectangle((40, 40, 984, 984), radius=220, fill=255)
    tile.putalpha(mask)
    d = ImageDraw.Draw(tile)
    d.rounded_rectangle((42, 42, 982, 982), radius=218, outline=(168, 173, 220, 105), width=3)

    # RC003-inspired silver body, widened and shortened for small icon sizes.
    # Drawn geometry only; no pixels from the product photo are incorporated.
    device = Image.new("RGBA", tile.size)
    body_mask = Image.new("L", tile.size)
    ImageDraw.Draw(body_mask).rounded_rectangle((334, 173, 654, 863), radius=30, fill=255)
    body = gradient((239, 238, 231), (158, 165, 174))
    metal = ImageDraw.Draw(body)
    for x in range(334, 655):
        t = (x - 334) / 320
        edge = math.exp(-t * 38) + math.exp(-(1-t) * 38)
        sheen = math.exp(-((t - .16) / .09) ** 2)
        for y in range(173, 864):
            v = round(219 - 24 * (y - 173) / 690 - 88 * edge + 22 * sheen)
            metal.point((x, y), fill=(v, v, min(255, v + 2), 255))
    body.putalpha(body_mask)
    device = Image.alpha_composite(device, body)
    d = ImageDraw.Draw(device)
    d.rounded_rectangle((337, 176, 651, 860), radius=28, outline=(239, 240, 242), width=3)

    ink, glyph = (32, 35, 39), (234, 237, 240)
    # Top power and microphone buttons, matching the real controller layout.
    for x in (409, 579):
        d.ellipse((x-33, 211, x+33, 277), fill=(203, 206, 207), outline=ink, width=3)
    d.arc((394, 229, 424, 260), 310, 230, fill=ink, width=5)
    d.line((409, 225, 409, 246), fill=ink, width=5)
    d.rounded_rectangle((572, 225, 586, 248), radius=7, fill=ink)
    d.arc((565, 232, 593, 257), 0, 180, fill=ink, width=4)
    d.line((579, 256, 579, 264), fill=ink, width=4)

    # Large dark navigation ring: the strongest identifying silhouette.
    d.ellipse((361, 296, 627, 562), fill=(38, 40, 43), outline=(239, 240, 242), width=3)
    d.arc((366, 301, 622, 557), 185, 290, fill=(89, 92, 97), width=4)
    d.ellipse((426, 361, 562, 497), fill=(48, 50, 53), outline=(13, 15, 18), width=4)

    # Back/home/menu on the left; the volume rocker and TV key on the right.
    for y in (614, 704, 794):
        d.ellipse((384, y-36, 456, y+36), fill=ink, outline=(245, 245, 243), width=2)
    d.rounded_rectangle((522, 578, 594, 740), radius=36, fill=ink, outline=(245, 245, 243), width=2)
    d.ellipse((522, 758, 594, 830), fill=ink, outline=(245, 245, 243), width=2)
    d.line([(426, 602), (414, 614), (426, 626)], fill=glyph, width=4)
    d.line([(407, 703), (420, 691), (433, 703), (433, 718), (407, 718), (407, 703)], fill=glyph, width=3)
    for y in (785, 794, 803):
        d.line((408, y, 432, y), fill=glyph, width=3)
    d.line((545, 614, 571, 614), fill=glyph, width=4)
    d.line((558, 601, 558, 627), fill=glyph, width=4)
    d.line((545, 704, 571, 704), fill=glyph, width=4)
    d.rounded_rectangle((541, 781, 575, 807), radius=5, outline=glyph, width=2)
    d.line((546, 788, 556, 788), fill=glyph, width=2)
    d.line((551, 788, 551, 800), fill=glyph, width=2)
    d.line([(560, 788), (564, 800), (569, 788)], fill=glyph, width=2)
    device = device.rotate(-17, resample=Image.Resampling.BICUBIC)

    shadow = Image.new("RGBA", tile.size, (0, 0, 0, 0))
    shadow.putalpha(device.getchannel("A").point(lambda a: round(a * .52)))
    shadow = shadow.filter(ImageFilter.GaussianBlur(22))
    tile.alpha_composite(shadow, (5, 22))
    tile = Image.alpha_composite(tile, device)

    # Two broad voice/radio arcs, large enough to survive 16px reduction.
    signal = Image.new("RGBA", tile.size)
    d = ImageDraw.Draw(signal)
    for box, width, color in [
        ((606, 226, 762, 382), 28, (113, 238, 203, 255)),
        ((558, 178, 810, 430), 25, (146, 155, 255, 255)),
    ]:
        d.arc(box, 290, 350, fill=color, width=width)
        cx, cy = (box[0] + box[2]) / 2, (box[1] + box[3]) / 2
        r = (box[2] - box[0] - width) / 2
        for angle in (290, 350):
            x = cx + r * math.cos(math.radians(angle))
            y = cy + r * math.sin(math.radians(angle))
            d.ellipse((x-width/2, y-width/2, x+width/2, y+width/2), fill=color)
    tile = Image.alpha_composite(tile, signal)
    # Clip antialiased shadow tails to the rounded tile; outside remains alpha.
    tile.putalpha(mask)
    return tile


def preview(icon, path):
    board = Image.new("RGB", (1200, 760), (12, 16, 26))
    d = ImageDraw.Draw(board)
    font = ImageFont.truetype("C:/Windows/Fonts/segoeui.ttf", 25)
    title = ImageFont.truetype("C:/Windows/Fonts/segoeuib.ttf", 40)
    d.text((52, 35), "SayAll / Windows", font=title, fill=(239, 242, 250))
    d.text((54, 92), "REMOTE + VOICE", font=font, fill=(111, 224, 189))
    board.paste(icon.resize((500, 500), Image.Resampling.LANCZOS), (38, 180), icon.resize((500, 500), Image.Resampling.LANCZOS))
    for y, background, foreground, label in [
        (204, (28, 32, 47), (231, 235, 245), "Dark"),
        (452, (238, 241, 248), (35, 43, 61), "Light"),
    ]:
        d.rounded_rectangle((592, y, 1150, y+208), radius=24, fill=background)
        d.text((616, y+16), label + " / actual pixel sizes", font=font, fill=foreground)
        for x, size in zip((638, 726, 814, 914, 1040), (16, 24, 32, 48, 64)):
            sample = icon.resize((size, size), Image.Resampling.LANCZOS)
            board.paste(sample, (x-size//2, y+101-size//2), sample)
            d.text((x-15, y+151), str(size), font=font, fill=foreground)
    path.parent.mkdir(parents=True, exist_ok=True)
    board.save(path)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--preview", type=Path)
    args = parser.parse_args()
    icon = master()
    assets = {ROOT / "public/app-logo.png": 512, ROOT / "public/favicon.png": 32}
    assets.update({ROOT / "src-tauri/icons" / name: size for name, size in PNG_SIZES.items()})
    for path, size in assets.items():
        icon.resize((size, size), Image.Resampling.LANCZOS).save(path)
    ico_path = ROOT / "src-tauri/icons/icon.ico"
    icon.save(ico_path, sizes=[(size, size) for size in ICO_SIZES])
    for path, size in assets.items():
        with Image.open(path) as image:
            image.load()
            assert image.mode == "RGBA" and image.size == (size, size), path.name
            assert image.getextrema()[3][0] == 0 and image.getextrema()[3][1] == 255, path.name
    with Image.open(ico_path) as image:
        assert image.ico.sizes() == {(size, size) for size in ICO_SIZES}
        for size in ICO_SIZES:
            assert image.ico.getimage((size, size)).size == (size, size)
    if args.preview:
        preview(icon, args.preview)
    print(json.dumps({"result": "passed", "png_assets": len(assets), "ico_sizes": ICO_SIZES, "master": SIZE}))


if __name__ == "__main__":
    main()
