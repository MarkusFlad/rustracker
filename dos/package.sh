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
# variants for tracking down emulator problems (index.html?b=NAME)
variant() {
    sed "$2" web/dosbox.conf > "$TMP"/.jsdos/dosbox.conf
    (cd "$TMP" && zip -q -9 -r "$OUT"/rtrack-$1.jsdos RTRACK.COM RTRACK.BIN NEON.MOD .jsdos)
}
variant prompt 's/^RTRACK.COM$/echo Start mit RTRACK/'
variant max 's/^cycles=fixed 50000$/cycles=max/'
variant fast 's/^cycles=fixed 50000$/cycles=fixed 150000/'
variant dynamic 's/^core=normal$/core=dynamic/'
variant mem16 's/^memsize=32$/memsize=16/'
cp web/index.html "$OUT"/
rm -rf "$TMP"
ls -l "$OUT"
