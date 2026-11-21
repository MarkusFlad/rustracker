#!/bin/sh
# Runs RTRACK.COM for 8 seconds of playback in DOSBox (SB16, XMS, no window) and checks the
# message it prints on exit; takes a screenshot after 6 s (dosbox.png). Needs dosbox, Xvfb
# and ImageMagick (import).
set -e
cd "$(dirname "$0")/out"
cat > test.conf <<CONF
[sdl]
output=surface
[dosbox]
machine=svga_s3
memsize=32
[cpu]
core=dynamic
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
set RTRACK=T8
RTRACK.COM > RESULT.TXT
exit
CONF
rm -f RESULT.TXT dosbox.log
touch RESULT.TXT
# own X server, so a screenshot can be taken while RTRACK plays
Xvfb :77 -screen 0 1024x768x24 >/dev/null 2>&1 &
XPID=$!
sleep 1
DISPLAY=:77 SDL_AUDIODRIVER=dummy timeout 180 dosbox -conf test.conf > dosbox.log 2>&1 &
DPID=$!
sleep 6
DISPLAY=:77 import -window root dosbox.png || echo "no screenshot" >> dosbox.log
wait $DPID || echo "DOSBox: exit $?" >> dosbox.log
kill $XPID 2>/dev/null || true
cat dosbox.log
echo "--- RTRACK.COM said:"
cat RESULT.TXT
grep -q "Tschuess" RESULT.TXT
