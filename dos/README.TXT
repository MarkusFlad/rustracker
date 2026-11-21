RUSTRACKER fuer DOS
===================

Ein MOD-Player mit Pattern-Anzeige, Oszilloskop, Spektrum, Mandelbrot-, Wuerfel-,
Voxel-, See- und Amiga-Ansicht, geschrieben in Rust (ohne Standardbibliothek).

Aufruf:   RTRACK [datei.mod]      (ohne Namen: NEON.MOD)

Tasten:   Esc        Ende
          Leertaste  Pause
          S          Stopp (an den Anfang)
          Links/Rechts  vorige/naechste Position
          V          naechste Ansicht
          3          Spektrum 2D/3D
          P          Farbpalette (Mandelbrot, Voxel)
          R          Voxel grob/fein
          F          nur die Ansicht (ohne Leiste und Tabelle)

Voraussetzungen
  * 386 oder neuer, sinnvoll ab Pentium. Pattern-Tabelle, Oszilloskop, Spektrum
    und Amiga-Ansicht sind fuer einen Pentium 133 gedacht; die Mandelbrot-, Wuerfel-,
    Voxel- und See-Ansicht rechnen dort nur wenige Bilder pro Sekunde.
  * 16 MB RAM (das Programm braucht etwa 10 MB Erweiterungsspeicher)
  * VGA
  * Sound Blaster 16 (44,1 kHz Stereo, 16 Bit) oder Sound Blaster 2.0/Pro
    (22 kHz Mono, 8 Bit); die Einstellungen kommen aus der Variablen BLASTER,
    z. B.  SET BLASTER=A220 I5 D1 H5 T6
  * DOS im Real Mode: HIMEM.SYS ist in Ordnung, EMM386/QEMM nicht, und nicht in
    einer Windows-DOS-Box. Am einfachsten: beim Start F8 druecken und EMM386
    ueberspringen, oder eine Bootdiskette ohne Speichermanager.

Das Programm laeuft auch in DOSBox (sbtype=sb16, memsize=16 oder mehr).

Fuer automatische Tests: SET RTRACK=T10 beendet nach 10 Sekunden Wiedergabe.

Quelltext: https://github.com/MarkusFlad/rustracker
