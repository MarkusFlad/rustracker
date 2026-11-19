#!/usr/bin/env python3
"""Write www/song.js with a MOD file embedded as base64 (the page's default song).

Usage: python3 scripts/embed_song.py <song.mod> [www/song.js]

The page loads song.js instead of fetching the .mod file, because some hosts
(e.g. claude.ai artifacts) do not serve .mod files.
"""
import base64
import sys

if len(sys.argv) < 2:
    sys.exit(__doc__)
src = sys.argv[1]
dst = sys.argv[2] if len(sys.argv) > 2 else "www/song.js"
data = open(src, "rb").read()
if len(data) < 600:
    sys.exit(f"{src}: too short for a MOD file")
title = data[:20].split(b"\0")[0].decode("latin-1").strip() or src
b64 = base64.b64encode(data).decode()
with open(dst, "w") as f:
    f.write(f"// {title!r} ({src}), base64 - the default song of the page.\n")
    f.write(f'self.DEFAULT_SONG_B64 = "{b64}";\n')
print(f"{dst}: {title} ({len(data)} bytes)")
