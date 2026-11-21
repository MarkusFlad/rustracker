#!/usr/bin/env sh
# Renders the promo video: caption cards, footage drawn by the Rust renderer, the music of the
# demo song, and a thumbnail. Needs cargo, ffmpeg, Python 3 with Pillow and the DejaVu fonts.
# Usage: tools/promo/make_video.sh [song.mod] [out-dir]
set -e
cd "$(dirname "$0")/../.."
SONG=${1:-www/drozerix_-_neon_techno.mod}
OUT=${2:-target/promo}
LEN=61.4333   # 1843 frames at 30 fps (the end of the last scene)
mkdir -p "$OUT/cards"
cargo build --release --bin promo --bin mod2wav
python3 tools/promo/cards.py "$OUT/cards"
sed "s#CARDS/#$OUT/cards/#" tools/promo/scenes.txt > "$OUT/scenes.txt"
./target/release/promo "$SONG" "$OUT/scenes.txt" "$OUT" --stills 20,44
./target/release/mod2wav "$SONG" "$OUT/music.wav" 63 > /dev/null
( cd "$OUT" && for f in scene*.mp4; do echo "file '$f'"; done > list.txt )
ffmpeg -y -loglevel error -f concat -safe 0 -i "$OUT/list.txt" -c copy "$OUT/video.mp4"
ffmpeg -y -loglevel error -i "$OUT/video.mp4" -i "$OUT/music.wav" -map 0:v -map 1:a -c:v copy \
  -af "atrim=0:$LEN,afade=t=out:st=59.4:d=2.03" -c:a aac -b:a 192k -movflags +faststart "$OUT/rustracker_promo.tmp.mp4"
mv "$OUT/rustracker_promo.tmp.mp4" "$OUT/rustracker_promo.mp4"
python3 tools/promo/cards.py "$OUT" thumb "$OUT/still_020.0.ppm" "$OUT/still_044.0.ppm"
echo "Video: $OUT/rustracker_promo.mp4  Vorschaubild: $OUT/rustracker_thumbnail.jpg"
