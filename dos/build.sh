#!/bin/sh
# Builds the DOS edition into dos/out: RTRACK.COM (loader), RTRACK.BIN (32-bit image) and a
# test song. Needs rustc (nightly features via RUSTC_BOOTSTRAP), nasm and ld.lld.
#   dos/build.sh            build
#   dos/build.sh selftest   also build dos/out/selftest (i386 Linux harness without hardware)
set -e
cd "$(dirname "$0")"
OUT=out
mkdir -p $OUT
RUSTC_BOOTSTRAP=1 rustc --edition 2021 --target i586-unknown-linux-gnu --crate-type lib \
    --emit obj -C opt-level=3 -C panic=abort -C overflow-checks=no -C relocation-model=static \
    -C target-cpu=pentium --cfg dos -o $OUT/rtrack.o rtrack.rs
nasm -f elf32 entry.asm -o $OUT/entry.o
nasm -f elf32 rt.asm -o $OUT/rt.o
ld.lld -m elf_i386 -T link.ld --oformat binary $OUT/entry.o $OUT/rtrack.o $OUT/rt.o -o $OUT/RTRACK.BIN
nasm -f bin stub.asm -o $OUT/RTRACK.COM
cp ../www/drozerix_-_neon_techno.mod $OUT/NEON.MOD
if [ "$1" = selftest ]; then
    nasm -f elf32 selftest.asm -o $OUT/selftest.o
    ld.lld -m elf_i386 -static -e _start $OUT/selftest.o $OUT/rtrack.o $OUT/rt.o -o $OUT/selftest
fi
ls -l $OUT/RTRACK.COM $OUT/RTRACK.BIN
