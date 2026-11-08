#!/usr/bin/env python3
"""Soju logo files, share images and watermarked screenshots/videos for docs/assets.

    python3 site/brand.py            (needs Pillow; ffmpeg for the videos)

Inputs: site/assets-src/ (unwatermarked originals, logo-1024.png from the app icon).
Outputs go to docs/assets/. Re-run after replacing an original.
"""
import os
from pathlib import Path
import shutil
import subprocess
from PIL import Image, ImageDraw, ImageFont, ImageFilter

ROOT = Path(__file__).resolve().parent
SRC = ROOT / 'assets-src'
OUT = ROOT.parent / 'docs' / 'assets'
BG = (18, 20, 19)


def font(size, bold=False):
    for path in ['/System/Library/Fonts/SFNS.ttf', '/System/Library/Fonts/Helvetica.ttc', '/Library/Fonts/Arial.ttf']:
        if os.path.exists(path):
            try:
                f = ImageFont.truetype(path, size)
                if bold and path.endswith('SFNS.ttf'):
                    try: f.set_variation_by_name('Bold')
                    except Exception: pass
                return f
            except OSError:
                continue
    return ImageFont.load_default()


def app_icon():
    """The app icon without its transparent margin (used where a solid tile is required)."""
    icon = Image.open(SRC / 'logo-1024.png').convert('RGBA')
    return icon.crop(icon.getchannel('A').getbbox())


def logo():
    """The bottle-and-glass drawing on a transparent background, centred on a square canvas."""
    art = Image.open(SRC / 'logo-mark.png').convert('RGBA')
    side = max(art.size)
    square = Image.new('RGBA', (side, side), (0, 0, 0, 0))
    square.alpha_composite(art, ((side - art.width) // 2, (side - art.height) // 2))
    return square


def logo_files(mark):
    for size, name in [(512, 'soju-logo.png'), (256, 'favicon.png'), (64, 'soju-logo-64.png')]:
        mark.resize((size, size), Image.LANCZOS).save(OUT / name)
    # The app sidebar uses the same transparent drawing.
    mark.resize((256, 256), Image.LANCZOS).save(ROOT.parent / 'resources' / 'soju-mark.png', optimize=True)
    # iOS fills transparent touch icons with black, so the home-screen icon keeps the solid app tile.
    app_icon().resize((180, 180), Image.LANCZOS).convert('RGB').save(OUT / 'apple-touch-icon.png')


def watermark_layer(width, mark):
    """Logo plus 'Soju · didimlabs.com' with a soft shadow instead of a backing box."""
    h = max(28, width // 30)
    small = mark.resize((h, h), Image.LANCZOS)
    f = font(int(h * 0.46), bold=True)
    text = 'Soju · didimlabs.com'
    tw = int(f.getlength(text))
    pad = h // 3
    layer = Image.new('RGBA', (h + pad + tw + pad, h + pad), (0, 0, 0, 0))
    layer.alpha_composite(small, (pad // 2, pad // 2))
    shadow = Image.new('RGBA', layer.size, (0, 0, 0, 0))
    ImageDraw.Draw(shadow).text((h + pad // 2 + 1, layer.height // 2 + 2), text, font=f, fill=(0, 0, 0, 200), anchor='lm')
    shadow = shadow.filter(ImageFilter.GaussianBlur(max(2, h // 12)))
    layer.alpha_composite(shadow)
    ImageDraw.Draw(layer).text((h + pad // 2, layer.height // 2), text, font=f, fill=(244, 245, 242, 240), anchor='lm')
    return layer


def watermark_image(name, mark, quality=88):
    img = Image.open(SRC / name).convert('RGBA')
    layer = watermark_layer(img.width, mark)
    margin = max(12, img.width // 60)
    img.alpha_composite(layer, (img.width - layer.width - margin, img.height - layer.height - max(margin, img.height // 14)))
    out = OUT / name
    if name.endswith('.jpg'):
        img.convert('RGB').save(out, quality=quality, optimize=True)
    else:
        img.convert('RGB').save(out, optimize=True)


def share_image(mark):
    """1200x630 preview used by og:image on the Soju pages."""
    img = Image.new('RGB', (1200, 630), BG)
    shot = Image.open(SRC / 'soju-library.png').convert('RGB')
    shot.thumbnail((700, 480))
    frame = Image.new('RGB', (shot.width + 2, shot.height + 2), (44, 49, 45))
    frame.paste(shot, (1, 1))
    img.paste(frame, (1200 - frame.width - 50, (630 - frame.height) // 2 + 40))
    m = mark.resize((150, 150), Image.LANCZOS)
    img.paste(m, (60, 70), m)
    d = ImageDraw.Draw(img)
    d.text((60, 250), 'Soju', font=font(84, bold=True), fill=(231, 233, 230))
    lines = ['Windows game launchers', 'on Apple Silicon Macs', '', 'Steam · Battle.net · Epic', 'GOG · Xbox PC', '', 'didimlabs.com']
    y = 360
    for line in lines:
        d.text((62, y), line, font=font(26), fill=(166, 173, 167) if line != 'didimlabs.com' else (92, 192, 141))
        y += 34 if line else 14
    img.save(OUT / 'soju-share.png', optimize=True)


def watermark_video(name, mark):
    src, out = SRC / name, OUT / name
    probe = subprocess.run(['ffprobe', '-v', 'error', '-select_streams', 'v:0', '-show_entries', 'stream=width', '-of', 'csv=p=0', str(src)],
                           capture_output=True, text=True, check=True)
    width = int(probe.stdout.strip().strip(',').split(',')[0])
    tmp = ROOT / '.wm.png'
    watermark_layer(width, mark).save(tmp)
    margin = max(12, width // 60)
    # The original recordings end with the old soju.snack-wrap.com caption in the bottom-right corner:
    # paint it over with the frame's background colour, then lay the watermark on top for every frame.
    cover = f'drawbox=x=iw-330:y=ih-60:w=320:h=48:color=0x101915:t=fill'
    subprocess.run(['ffmpeg', '-y', '-v', 'error', '-i', str(src), '-loop', '1', '-i', str(tmp), '-filter_complex',
                    f'[0:v]{cover}[bg];[bg][1:v]overlay=W-w-{margin}:H-h-{margin}:shortest=1', '-c:v', 'libx264', '-crf', '21', '-preset', 'medium',
                    '-pix_fmt', 'yuv420p', '-c:a', 'copy', '-movflags', '+faststart', str(out)], check=True)
    tmp.unlink()


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    mark = logo()
    logo_files(mark)
    for name in ['soju-library.png', 'soju-discover.png', 'soju-desktop.png', 'soju-store.jpg', 'soju-sponsor-slot.png',
                 'soju-gameplay-demo-poster.jpg', 'soju-launcher-demo-poster.jpg']:
        if (SRC / name).exists():
            watermark_image(name, mark)
    share_image(mark)
    if shutil.which('ffmpeg'):
        for name in ['soju-gameplay-demo.mp4', 'soju-launcher-demo.mp4']:
            if (SRC / name).exists():
                watermark_video(name, mark)
    print('brand assets written to', OUT)


if __name__ == '__main__':
    main()
