#!/usr/bin/env python3
"""Runs the DOS build's Linux self-test (dos/out/selftest) for every view and checks that each
last frame has content. Writes the frames as PNG (2x) to the given directory.

    python3 dos/selftest_check.py SONG.MOD OUTDIR [SECONDS]
"""
import pathlib, subprocess, sys
from PIL import Image

song, outdir = sys.argv[1], pathlib.Path(sys.argv[2])
secs = sys.argv[3] if len(sys.argv) > 3 else "4"
outdir.mkdir(parents=True, exist_ok=True)
names = ["osci", "mandel", "spektrum", "wuerfel", "voxel", "see", "demo", None, "spektrum3d"]
failed = False
for view, name in enumerate(names):
    if name is None:
        continue
    raw = outdir / f"{name}.raw"
    r = subprocess.run(["dos/out/selftest", song, str(raw), secs, str(view)])
    data = raw.read_bytes()
    raw.unlink()
    px, pal = data[:64000], data[64000:]
    colours = len(set(px))
    lit = sum(1 for p in px if p != 0)
    ok = r.returncode != 255 and colours >= 8 and lit > 3000
    failed |= not ok
    print(f"view {view} {name:9s} exit {r.returncode:3d}  colours {colours:3d}  lit {lit:5d}  {'ok' if ok else 'FEHLER'}")
    im = Image.frombytes("P", (320, 200), px)
    im.putpalette(pal)
    im.convert("RGB").resize((640, 400), Image.NEAREST).save(outdir / f"{name}.png")
sys.exit(1 if failed else 0)
