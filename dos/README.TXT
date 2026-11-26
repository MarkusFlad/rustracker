RUSTRACKER for DOS
==================

A MOD player with pattern display, oscilloscope, spectrum, Mandelbrot, cube,
voxel, lake and demo scene views, written in Rust (without a standard library).

Usage:    RTRACK [options] [file.mod]      (without a name: NEON.MOD)

Options:  /M   add the Mandelbrot zoom
          /W   add the cube
          /V   add the voxel landscape (3D Mandelbrot)
          /B   benchmark: every view for 15 s, then a table of frames per
               second on the screen and in BENCH.TXT; on a Pentium BENCH.TXT
               and the messages after Esc also show where the time goes
          /H   sound at 44.1 kHz instead of 22 kHz (Sound Blaster 16; mixing
               at 44.1 kHz takes about 40 % of a Pentium 133)
          /?   help
          These views are too demanding for a Pentium 133 (Mandelbrot and
          voxel take several seconds per frame), so they are left out unless
          asked for. Example: RTRACK /M /W /V NEON.MOD

Keys:     Esc        quit
          Space      pause
          S          stop (back to the start)
          Left/Right previous/next position
          V          next view (only the enabled ones)
          3          spectrum 2D/3D
          P          colour palette (Mandelbrot, voxel)
          R          voxels coarse/fine
          F          view only (without bar and table)
          B          benchmark (B again cancels, any key closes the table)

Requirements
  * 386 or newer, a Pentium makes sense. Pattern table, oscilloscope,
    spectrum and demo scene are meant for a Pentium 133; the lake builds
    itself in a few seconds, and Mandelbrot, cube and voxel views manage only
    a few frames per second there.
  * 16 MB RAM (the program needs about 10 MB of extended memory)
  * VGA
  * Sound Blaster 16 (22 kHz stereo, 16 bit; 44.1 kHz with /H) or Sound Blaster 2.0/Pro
    (22 kHz mono, 8 bit). The settings come from the BLASTER variable, e.g.
    SET BLASTER=A220 I5 D1 H5 T6; a Sound Blaster 16 also reports its IRQ and
    DMA itself.
  * DOS in real mode: HIMEM.SYS is fine, EMM386/QEMM are not, and neither is a
    Windows DOS box. Easiest: press F8 while booting and skip EMM386, or use a
    boot floppy without a memory manager.

Tested on a Pentium 133 with 96 MB RAM and a Sound Blaster 16 under MS-DOS
5.0 and in the MS-DOS mode of Windows 98.

The program also runs in DOSBox: dosbox -conf dosbox.conf (included).

For automatic tests: SET RTRACK=T10 quits after 10 seconds of playback,
SET RTRACK=B3 runs the benchmark with 3 seconds per view and then quits.

Source code: https://github.com/MarkusFlad/rustracker
