#!/bin/sh
# Runs RTRACK.COM for 8 seconds of playback in DOSBox (SB16, XMS, no window) and checks the
# message it prints on exit. Needs dosbox and xvfb-run.
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
rm -f RESULT.TXT
SDL_AUDIODRIVER=dummy timeout 180 xvfb-run -a dosbox -conf test.conf || true
echo "--- RTRACK.COM said:"
cat RESULT.TXT
grep -q "Bloecke" RESULT.TXT
