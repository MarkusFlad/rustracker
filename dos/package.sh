#!/bin/sh
# Packs the built DOS edition (dos/build.sh) for the web site: rtrack-dos.zip (RTRACK.COM,
# RTRACK.BIN, NEON.MOD, README.TXT and a DOSBox configuration) and the page.
#   dos/package.sh OUTDIR
set -e
OUT=$(realpath -m "$1")
cd "$(dirname "$0")"
mkdir -p "$OUT"
TMP=$(mktemp -d)
cp out/RTRACK.COM out/RTRACK.BIN out/NEON.MOD web/dosbox.conf "$TMP"/
sed 's/$/\r/' README.TXT > "$TMP"/README.TXT
rm -f "$OUT"/rtrack-dos.zip
(cd "$TMP" && zip -q -9 "$OUT"/rtrack-dos.zip RTRACK.COM RTRACK.BIN NEON.MOD README.TXT dosbox.conf)
cp web/index.html "$OUT"/
rm -rf "$TMP"
ls -l "$OUT"
