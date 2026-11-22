#!/bin/sh
# Runs RTRACK.COM in DOSBox (SB16, XMS, no window) and checks the message it prints on exit.
#   dos/dosbox_test.sh [CORE] [SECONDS] [NAME]     defaults: dynamic 8 dosbox
# Writes, in dos/out: NAME-result.txt (what RTRACK.COM printed), NAME.log (DOSBox output,
# including any "Exit to error"), NAME-1.png (screenshot after 6 s) and NAME-2.png (3 s
# before the end). Needs dosbox, Xvfb and ImageMagick (import).
set -e
CORE=${1:-dynamic}
SECS=${2:-8}
NAME=${3:-dosbox}
cd "$(dirname "$0")/out"
cat > $NAME.conf <<CONF
[sdl]
output=surface
[dosbox]
machine=svga_s3
memsize=32
[cpu]
core=$CORE
cycles=max
[mixer]
rate=44100
[sblaster]
sbtype=sb16
sbbase=220
irq=5
dma=1
hdma=5
[dos]
xms=true
ems=false
[autoexec]
mount c .
c:
set RTRACK=T$SECS
RTRACK.COM > RESULT.TXT
exit
CONF
rm -f RESULT.TXT $NAME.log $NAME-result.txt $NAME-1.png $NAME-2.png
touch RESULT.TXT
# own X server, so screenshots can be taken while RTRACK plays
Xvfb :77 -screen 0 1024x768x24 >/dev/null 2>&1 &
XPID=$!
sleep 1
DISPLAY=:77 SDL_AUDIODRIVER=dummy timeout $((SECS * 4 + 60)) dosbox -conf $NAME.conf > $NAME.log 2>&1 &
DPID=$!
sleep 6
DISPLAY=:77 import -window root $NAME-1.png || echo "no screenshot 1" >> $NAME.log
if [ "$SECS" -gt 12 ]; then
    sleep $((SECS - 9))
    DISPLAY=:77 import -window root $NAME-2.png || echo "no screenshot 2" >> $NAME.log
fi
wait $DPID || echo "DOSBox: exit $?" >> $NAME.log
kill $XPID 2>/dev/null || true
sleep 1
mv RESULT.TXT $NAME-result.txt
cat $NAME.log
echo "--- RTRACK.COM said:"
cat $NAME-result.txt
grep -q "Tschuess" $NAME-result.txt
