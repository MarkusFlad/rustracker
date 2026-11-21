#!/bin/sh
# Packs the built DOS edition (dos/build.sh) for the web site: rtrack-dos.zip for real PCs,
# rtrack.jsdos (the same files plus a DOSBox configuration) and the js-dos page.
#   dos/package.sh OUTDIR
set -e
OUT=$(realpath -m "$1")
cd "$(dirname "$0")"
mkdir -p "$OUT"
TMP=$(mktemp -d)
cp out/RTRACK.COM out/RTRACK.BIN out/NEON.MOD "$TMP"/
sed 's/$/\r/' README.TXT > "$TMP"/README.TXT
rm -f "$OUT"/rtrack-dos.zip "$OUT"/rtrack.jsdos
(cd "$TMP" && zip -q -9 "$OUT"/rtrack-dos.zip RTRACK.COM RTRACK.BIN NEON.MOD README.TXT)
mkdir -p "$TMP"/.jsdos
cp web/dosbox.conf "$TMP"/.jsdos/dosbox.conf
(cd "$TMP" && zip -q -9 -r "$OUT"/rtrack.jsdos RTRACK.COM RTRACK.BIN NEON.MOD .jsdos)
cp web/index.html "$OUT"/
rm -rf "$TMP"
ls -l "$OUT"
