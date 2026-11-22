"""Caption cards and thumbnail of the promo video (see make_video.sh).

Usage: cards.py <out-dir>              draws the cards (1920x1080 RGBA PNGs)
       cards.py <out-dir> thumb A B    thumbnail from two stills (left and right half)
Needs Pillow and the DejaVu fonts.
"""
import sys, os
from PIL import Image, ImageDraw, ImageFont, ImageFilter
D = next((d for d in ['/usr/share/fonts/truetype/dejavu/', '/usr/share/fonts/dejavu/', '/Library/Fonts/']
          if os.path.exists(d + 'DejaVuSans-Bold.ttf')), '/usr/share/fonts/truetype/dejavu/')
SANS_B, MONO_B, MONO = D + 'DejaVuSans-Bold.ttf', D + 'DejaVuSansMono-Bold.ttf', D + 'DejaVuSansMono.ttf'
GREEN, AMBER, WHITE = (57, 255, 136), (255, 180, 58), (236, 248, 240)
OUT = sys.argv[1].rstrip('/') + '/'
W, H = 1920, 1080

def font(p, s): return ImageFont.truetype(p, s)

def glow_text(img, xy, text, f, fill, glow=10, anchor='la'):
    """Text with a soft glow of its own colour (phosphor look)."""
    layer = Image.new('RGBA', img.size, (0, 0, 0, 0))
    d = ImageDraw.Draw(layer)
    d.text(xy, text, font=f, fill=fill + (180,), anchor=anchor)
    layer = layer.filter(ImageFilter.GaussianBlur(glow))
    img.alpha_composite(layer)
    ImageDraw.Draw(img).text(xy, text, font=f, fill=fill + (255,), anchor=anchor)

def lower_third(name, num, title, sub=None, top=None):
    """Caption box at the bottom left (or with its top edge at `top`)."""
    img = Image.new('RGBA', (W, H), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    ft, fk, fs = font(SANS_B, 58), font(MONO_B, 26), font(MONO, 30)
    tw = d.textlength(title, font=ft)
    sw = d.textlength(sub, font=fs) if sub else 0
    bw = int(max(tw, sw, 420)) + 110
    bh = 190 if sub else 150
    x0, y0 = 90, (H - 110 - bh if top is None else top)
    panel = Image.new('RGBA', (W, H), (0, 0, 0, 0))
    ImageDraw.Draw(panel).rounded_rectangle((x0, y0, x0 + bw, y0 + bh), radius=14, fill=(4, 10, 8, 200))
    img.alpha_composite(panel)
    d.rectangle((x0, y0 + 18, x0 + 8, y0 + bh - 18), fill=GREEN + (255,))
    d.text((x0 + 44, y0 + 26), f'RUSTRACKER · {num:02d}', font=fk, fill=GREEN + (230,))
    glow_text(img, (x0 + 42, y0 + 62), title, ft, WHITE, glow=6)
    if sub:
        d.text((x0 + 44, y0 + 138), sub, font=fs, fill=(185, 216, 198, 255))
    img.save(OUT + name)

def title_card(name):
    img = Image.new('RGBA', (W, H), (0, 0, 0, 0))
    band = Image.new('RGBA', (W, H), (0, 0, 0, 0))
    bd = ImageDraw.Draw(band)
    for y in range(300, 780):
        a = int(170 * (1 - abs(y - 540) / 240) ** 0.6)
        bd.line((0, y, W, y), fill=(2, 6, 4, a))
    img.alpha_composite(band)
    glow_text(img, (W // 2, 470), 'RUSTRACKER', font(MONO_B, 170), GREEN, glow=18, anchor='mm')
    d = ImageDraw.Draw(img)
    d.text((W // 2, 610), 'Ein ProTracker-MOD-Player in Rust & WebAssembly', font=font(SANS_B, 46), fill=WHITE + (255,), anchor='mm')
    d.text((W // 2, 680), 'mit Echtzeit-Visualisierungen im Takt der Musik', font=font(MONO, 32), fill=(185, 216, 198, 255), anchor='mm')
    img.save(OUT + name)

def end_card(name):
    img = Image.new('RGBA', (W, H), (2, 6, 4, 195))
    glow_text(img, (W // 2, 330), 'RUSTRACKER', font(MONO_B, 150), GREEN, glow=16, anchor='mm')
    d = ImageDraw.Draw(img)
    d.text((W // 2, 460), 'Jetzt im Browser ausprobieren – auch auf dem Handy', font=font(SANS_B, 46), fill=WHITE + (255,), anchor='mm')
    f = font(MONO_B, 40)
    d.text((W // 2, 580), '▶  markusflad.github.io/rustracker', font=f, fill=GREEN + (255,), anchor='mm')
    d.text((W // 2, 650), '{ }  github.com/MarkusFlad/rustracker', font=f, fill=AMBER + (255,), anchor='mm')
    d.text((W // 2, 800), 'Musik: Drozerix – „Neon Techno“ (Public Domain)', font=font(MONO, 28), fill=(150, 175, 160, 255), anchor='mm')
    img.save(OUT + name)

def thumbnail(name, left, right):
    """1280x720 thumbnail: two stills side by side with the title."""
    img = Image.new('RGBA', (1280, 720))
    img.paste(Image.open(left).convert('RGBA').resize((1280, 720), Image.LANCZOS).crop((0, 0, 640, 720)), (0, 0))
    img.paste(Image.open(right).convert('RGBA').resize((1280, 720), Image.LANCZOS).crop((320, 0, 960, 720)), (640, 0))
    shade = Image.new('RGBA', (1280, 720), (0, 0, 0, 0))
    d = ImageDraw.Draw(shade)
    for y in range(720):
        d.line((0, y, 1280, y), fill=(2, 6, 4, int(200 * max(0, 1 - abs(y - 330) / 260))))
    img.alpha_composite(shade)
    glow_text(img, (640, 290), 'RUSTRACKER', font(MONO_B, 140), GREEN, glow=14, anchor='mm')
    ImageDraw.Draw(img).text((640, 410), 'MOD-Player in Rust + WebAssembly', font=font(SANS_B, 52), fill=(240, 250, 244, 255), anchor='mm')
    img.convert('RGB').save(OUT + name, quality=92)

if len(sys.argv) > 2 and sys.argv[2] == 'thumb':
    thumbnail('rustracker_thumbnail.jpg', sys.argv[3], sys.argv[4])
    sys.exit(0)

os.makedirs(OUT, exist_ok=True)
title_card('title.png')
lower_third('c1.png', 1, 'Echtzeit-FFT, selbst geschrieben', 'Balken, Spitzenwerte, Spektrogramm')
lower_third('c2.png', 2, 'Das Spektrum in 3D', 'die letzten 4 Sekunden in die Tiefe')
lower_third('c3.png', 3, 'Mandelbrot-Zoom im Takt', 'jede Pattern-Position eine neue Reise')
lower_third('c4.png', 4, 'Sechs Ansichten auf einem Würfel', 'alle live, perspektivisch korrekt')
lower_third('c5.png', 5, 'Das Fraktal als Voxel-Landschaft', 'Kameraflug um die Gipfel')
lower_third('c6.png', 6, 'Ein See, den der Bass bewegt', 'echte Wellengleichung, Wellen überlagern sich')
lower_third('c7.png', 8, 'Jedes Pixel in Rust gezeichnet', 'ohne Frameworks, ohne fremde Crates')
lower_third('c8.png', 7, 'Eine Verbeugung vor der Demoszene', 'C64, Amiga und PC: Rasterzeilen, Copper-Bars, Plasma', top=330)  # keeps the scroller free
end_card('end.png')
print('ok')
