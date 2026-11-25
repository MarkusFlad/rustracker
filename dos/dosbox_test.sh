#!/bin/sh
# Runs RTRACK.COM in DOSBox (SB16, XMS, no window) and checks the message it prints on exit.
#   dos/dosbox_test.sh [CORE] [SECONDS] [NAME] [bench]     defaults: dynamic 8 dosbox
# With "bench" RTRACK runs its benchmark with SECONDS per scene instead of playing SECONDS
# seconds, and NAME-bench.txt gets the table it writes to BENCH.TXT.
# Writes, in dos/out: NAME-result.txt (what RTRACK.COM printed), NAME.log (DOSBox output,
# including any "Exit to error"), NAME-1.png (screenshot after 6 s) and NAME-2.png (3 s
# before the end). Needs dosbox, Xvfb and ImageMagick (import).
set -e
CORE=${1:-dynamic}
SECS=${2:-8}
NAME=${3:-dosbox}
MODE=${4:-play}
CPUTYPE=auto
if [ "$MODE" = bench ]; then
    OPT=B$SECS
    CPUTYPE=pentium_slow            # with time stamp counter: details per stage
    LIMIT=$((SECS * 8 * 3 + 180))   # 8 scenes, plus the slow first frames of lake and zoom
else
    OPT=T$SECS
    LIMIT=$((SECS * 4 + 60))
fi
cd "$(dirname "$0")/out"
cat > $NAME.conf <<CONF
[sdl]
output=surface
[dosbox]
machine=svga_s3
memsize=32
[cpu]
core=$CORE
cputype=$CPUTYPE
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
set RTRACK=$OPT
RTRACK.COM > RESULT.TXT
exit
CONF
rm -f RESULT.TXT BENCH.TXT $NAME.log $NAME-result.txt $NAME-bench.txt $NAME-1.png $NAME-2.png
touch RESULT.TXT
# own X server, so screenshots can be taken while RTRACK plays
Xvfb :77 -screen 0 1024x768x24 >/dev/null 2>&1 &
XPID=$!
sleep 1
DISPLAY=:77 SDL_AUDIODRIVER=dummy timeout $LIMIT dosbox -conf $NAME.conf > $NAME.log 2>&1 &
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
grep -q "Bye!" $NAME-result.txt
if [ "$MODE" = bench ]; then
    mv BENCH.TXT $NAME-bench.txt
    grep -q "SCORE" $NAME-bench.txt
fi
