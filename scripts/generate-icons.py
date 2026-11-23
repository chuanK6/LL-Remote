from pathlib import Path
from PIL import Image, ImageDraw

root = Path(__file__).resolve().parent.parent / 'src-tauri' / 'icons'
root.mkdir(parents=True, exist_ok=True)
icon = Image.new('RGBA', (256, 256), (0, 0, 0, 0))
draw = ImageDraw.Draw(icon)
draw.rounded_rectangle((0, 0, 255, 255), 58, fill='#3785ff')
for x in (65, 145):
    draw.line([(x, 62), (x, 190), (x + 44, 190)], fill='white', width=22, joint='curve')
    for px, py in [(x, 62), (x, 190), (x + 44, 190)]:
        draw.ellipse((px - 10, py - 10, px + 10, py + 10), fill='white')
for size in (32, 128):
    icon.resize((size, size), Image.Resampling.LANCZOS).save(root / f'{size}x{size}.png')
icon.save(root / 'icon.ico', sizes=[(16, 16), (32, 32), (48, 48), (128, 128), (256, 256)])
