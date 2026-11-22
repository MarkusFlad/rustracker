# Die DOS-Edition des Rustrackers

Dieses Dokument beschreibt, wie der Rustracker auf einem PC unter DOS läuft: welche Teile in
Assembler geschrieben sind und warum, welche in Rust, wie das Programm geladen und gestartet
wird und welchen Weg Bild und Ton von Rust bis zur VGA-Karte und zum Sound Blaster nehmen.

Ziel war ein Pentium (gedacht ist ein Pentium 133) mit VGA (Modus 13h, 320 × 200 Pixel, 256
Farben) und Sound Blaster 16 oder 2.0/Pro, unter DOS ohne Speichermanager. In DOSBox läuft das
Programm genauso.

## Überblick

| Datei | Sprache | Aufgabe |
|---|---|---|
| `dos/stub.asm` | Assembler, 16 Bit | `RTRACK.COM`: der Lader. Läuft im Real Mode unter DOS, lädt alles, schaltet in den Protected Mode und wieder zurück |
| `dos/entry.asm` | Assembler, 32 Bit | Einstieg in `RTRACK.BIN`: Segmente, Interrupt-Tabelle, Interrupt-Controller, Interrupt-Einsprung, Port-Zugriffe |
| `dos/rt.asm` | Assembler, 32 Bit | `memcpy`, `memset`, `memmove`, `memcmp`, die der Compiler aufruft, und zwei Hilfen für Speicherzugriffe, die der Compiler nicht wegoptimieren darf |
| `dos/dos.rs` | Rust | die DOS-Schicht: Sound Blaster, DMA, Interrupt-Logik, Tastatur, Palette, VGA-Ausgabe, Kommandozeile, Meldungen |
| `dos/rtrack.rs` | Rust | Wurzel des Crates: bindet die Quellen aus `src/` ein und übersetzt sie mit `--cfg dos` |
| `src/*.rs` | Rust | Replayer, FFT, Framebuffer-Renderer mit allen Ansichten: dieselben Dateien wie im Browser |
| `dos/link.ld` | Linker-Skript | legt alles als flaches Abbild ab Adresse 0 ab |
| `dos/selftest.asm` | Assembler, 32 Bit | Testrahmen für Linux: lässt den Rust-Teil ohne Hardware laufen |
| `dos/build.sh`, `dos/package.sh` | Shell | bauen und packen |

Von rund 2 200 Zeilen in `dos/` sind etwa 1 100 Assembler (davon knapp 700 der Lader) und
etwa 1 100 Rust. Der weitaus größte Teil des Programms, Replayer und Renderer, kommt
unverändert aus `src/`.

## Warum eine .COM- und eine .BIN-Datei?

Das Programm braucht im Betrieb gut 9 MB Speicher: Der Renderer hält Puffer für Mandelbrot,
Voxel-Landschaft, See, Spektrogramm und das Bild selbst. Unter DOS stehen einem Programm aber
nur die unteren 640 KB direkt zur Verfügung, und im Real Mode sieht der Prozessor ohnehin nur
das erste Megabyte, in Stücken von 64 KB. Der Rest des Speichers, der Erweiterungsspeicher
oberhalb von 1 MB, ist nur im 32-Bit-Protected-Mode bequem nutzbar.

Üblich war dafür ein DOS-Extender: eine `.EXE` im LE- oder LX-Format mit einem Stub wie
DOS/4GW, gebaut mit Watcom C oder DJGPP. Für Rust gibt es keine solche Toolchain und kein
DOS-Target. Deshalb besteht der Rustracker aus zwei Teilen:

* **`RTRACK.COM`** ist der Lader, gut 2 KB groß. Das COM-Format ist das einfachste Programmformat
  von DOS: Die Datei wird ohne Kopf und ohne Relokation an Offset 100h eines Segments geladen und
  dort gestartet. Code und Daten müssen in 64 KB passen, was für den Lader reicht. Er ist in
  16-Bit-Assembler geschrieben, weil er DOS- und BIOS-Funktionen über Software-Interrupts
  aufruft (`int 21h`, `int 10h`, `int 15h`, `int 2Fh`), und das geht nur im Real Mode. Eine
  `.EXE` im MZ-Format hätte keinen Vorteil gebracht, nur einen Kopf mit Relokationstabelle, den
  der Lader nicht braucht.
* **`RTRACK.BIN`** ist das eigentliche Programm: ein flaches 32-Bit-Abbild ohne jedes
  Dateiformat, etwa 200 KB groß. Der Linker legt Code und Daten ab Adresse 0 ab; der Lader
  kopiert die Datei in den Erweiterungsspeicher und stellt die Segmente so ein, dass ihre Basis
  die Ladeadresse ist. Dann stimmen alle absoluten Adressen im Code, ohne dass etwas relokiert
  werden muss. Am Anfang der Datei steht ein kleiner Kopf (`RTRK`, Speicherbedarf, Dateigröße),
  den der Lader prüft.

Die beiden Teile könnte man auch zu einer Datei zusammenfügen (der Lader liest sich dann selbst
ab einem festen Offset). Getrennt sind sie leichter zu bauen und zu prüfen.

Einen DOS-Extender braucht der Rustracker nicht, weil er sich selbst in den Protected Mode
schaltet und dort bleibt, bis er beendet wird. Er ruft dazwischen weder DOS noch das BIOS auf.
Der Preis: Er läuft nur, wenn DOS im Real Mode läuft. EMM386, QEMM oder eine Windows-DOS-Box
versetzen den Prozessor in den Virtual-8086-Modus, aus dem man so nicht in den Protected Mode
kommt (dafür gäbe es die Schnittstellen VCPI und DPMI). Der Lader prüft das und bricht mit einer
Meldung ab. HIMEM.SYS stört nicht, im Gegenteil: Den Erweiterungsspeicher holt sich der Lader
darüber.

## Was in Assembler ist und warum

Assembler steckt nur dort, wo Rust nicht hinkommt.

**Der Lader (`stub.asm`, 16 Bit, Real Mode).** Rust kann keinen 16-Bit-Real-Mode-Code erzeugen,
und die Arbeit des Laders besteht fast nur aus Aufrufen von DOS und BIOS:

* Speicher ordnen (`int 21h`, 4Ah und 48h), Dateien öffnen und lesen (3Dh, 3Fh, 42h),
* Erweiterungsspeicher von HIMEM.SYS holen (XMS über `int 2Fh` 4310h, dann Funktionen 09h
  allozieren, 0Ch sperren und physische Adresse erfahren, 0Bh kopieren), ersatzweise über das
  BIOS (`int 15h` 88h und 87h),
* den Bildschirm in Modus 13h schalten (`int 10h`),
* die Umschaltung in den Protected Mode: Deskriptortabelle (GDT) laden, Bit PE im Register CR0
  setzen, weiter mit einem Fernsprung in ein 32-Bit-Codesegment,
* den Rückweg: 16-Bit-Segmente laden, PE löschen, Fernsprung in den Real Mode, die
  Interrupt-Tabelle des Real Mode und die Interrupt-Controller wiederherstellen.

**Der 32-Bit-Einstieg (`entry.asm`).** Der Code läuft im Protected Mode, aber manches kann Rust
hier nicht:

* Segmentregister laden und den Stapel einrichten, bevor irgendein Rust-Code läuft.
* Die Interrupt-Tabelle (IDT) aufbauen und mit `lidt` laden.
* Die Einsprungpunkte der Interrupts schreiben. Ein Interrupt-Handler muss alle Register retten,
  den FPU-Zustand sichern (`fsave`, `frstor`) und mit `iretd` zurückkehren statt mit `ret`.
  Rust kennt dafür nur die instabile Aufrufkonvention `x86-interrupt`, und die passt nicht zum
  Sprachkern-Subset, auf das sich das ganze Projekt beschränkt.
* Die Ports der Hardware ansprechen: `in` und `out`. In Rust ginge das mit Inline-Assembler
  (`asm!`), aber der gehört zur Bibliothek `core`, die das Projekt bewusst nicht benutzt (siehe
  unten). Also gibt es winzige Funktionen `inb`, `outb`, `irq_enable` (`sti`) und `irq_disable`
  (`cli`), die Rust wie normale C-Funktionen aufruft.
* Die Interrupt-Controller (zwei 8259) von den Vektoren 08h/70h auf 20h–2Fh umlegen, damit
  Hardware-Interrupts nicht mit den Prozessor-Ausnahmen 0–31 zusammenfallen.
* Prozessor-Ausnahmen abfangen: Bei einem Fehler kehrt das Programm zum Lader zurück, der Nummer,
  Adresse (EIP) und Fehlercode anzeigt, statt den Rechner hängen zu lassen.
* Den Wachhund ausführen: Meldet `rust_irq`, dass die Hauptschleife 30 Sekunden lang kein Bild
  fertig bekommen hat, kehrt der Einstieg direkt aus dem Interrupt zum Lader zurück. Der zeigt
  die Adresse, an der die Hauptschleife gerade war; mit der Linker-Map (`rtrack.map`) lässt
  sie sich einer Funktion zuordnen.

**Die Laufzeit (`rt.asm`).** Der Compiler erzeugt für Kopierschleifen und große Zuweisungen
Aufrufe von `memcpy` und `memset`. Normalerweise kommen die aus der C-Bibliothek oder aus
`compiler_builtins`; hier gibt es beides nicht, also stehen sie in ein paar Zeilen Assembler
(`rep movsd`, `rep stosb`). Dazu kommen `peek32` und `poke32`: Lese- und Schreibzugriffe auf
Variablen, die der Interrupt-Handler und das Hauptprogramm teilen, und die der Optimierer nicht
in Register verlegen darf.

## Was in Rust ist

**Gemeinsam mit dem Browser.** Replayer (`replayer.rs`), FFT (`fft.rs`), Spektrum und der
komplette Framebuffer-Renderer mit allen Ansichten, Knopfleiste und Pattern-Tabelle (`vis*.rs`,
`gfx.rs`, `voxel_space.rs` …) sind dieselben Quellen wie in der WebAssembly-Fassung.
`dos/rtrack.rs` bindet sie mit `#[path = "../src/..."]` ein. Das funktioniert, weil das Projekt
nur den Sprachkern von Rust benutzt: keine Standardbibliothek, nicht einmal `core`, sondern eine
eigene Minimalfassung der Sprachelemente (`preview/minicore_lang.rs`). Ob ein Prozessor Pentium
oder WebAssembly heißt, ist diesem Code egal.

Für DOS gibt es mit `--cfg dos` nur wenige Abweichungen in `src/`:

* kleinere Puffer (Framebuffer für 320 × 200 statt 1,1 Millionen Pixel, Platz für Songs bis 4 MB),
* Linien in 16.16-Festkomma statt in Gleitkomma (`gfx.rs`): Jede Umwandlung von Gleitkomma in
  eine ganze Zahl kostet auf der x87-FPU des Pentium viele Takte, weil dafür der Rundungsmodus
  umgeschaltet wird,
* höchstens ein Kurvenpunkt pro Pixelspalte im Oszilloskop (`vis_osc.rs`),
* die Linien des 3D-Spektrums ebenfalls in Festkomma (`vis_spec.rs`),
* Würfelflächen mit 128 × 128 statt 256 × 256 Pixeln und eine Mandelbrot-Fläche in halber
  Auflösung mit kleinerem Rechenanteil pro Bild (`vis.rs`, `vis_cube.rs`, `vis_mandel.rs`).

**Speziell für DOS (`dos/dos.rs`).**

* `rust_main`: Ablaufsteuerung von der Boot-Information bis zum Ende.
* Kommandozeile (`/M`, `/W`, `/V`, `/?`) und Umgebungsvariablen (`BLASTER`, `RTRACK`).
* Sound Blaster: DSP zurücksetzen, Version lesen, DMA-Controller und DSP programmieren, im
  Interrupt nachmischen, am Ende anhalten.
* Tastatur: direkt am Tastatur-Controller abgefragt.
* Bild: Palette, die dem Bild folgt, Umrechnung in Palettenindizes, Ausgabe im Strahlrücklauf.
* Meldungen: ein Text-Puffer im konventionellen Speicher, den der Lader nach dem Zurückschalten
  ausgibt.
* `rust_selftest`: derselbe Ablauf ohne Hardware für den Linux-Testrahmen.

Übersetzt wird mit dem normalen `rustc` für das Target `i586-unknown-linux-gnu` (32 Bit,
Pentium-Befehlssatz ohne MMX und SSE, Gleitkomma auf der x87-FPU) als reine Objektdatei. Linux
spielt dabei keine Rolle; es wird keine einzige Linux-Funktion aufgerufen. `nasm` übersetzt die
Assembler-Teile, `ld.lld` verbindet alles nach `dos/link.ld` zu einem flachen Abbild.

## Wie das Programm startet

```
DOS  ──>  RTRACK.COM (Real Mode, 16 Bit)
            1. Speicher an DOS zurückgeben, V86-Modus ausschließen
            2. RTRACK.BIN und den Song öffnen, Kopf prüfen
            3. ~10 MB Erweiterungsspeicher holen (XMS oder BIOS), A20-Leitung an
            4. RTRACK.BIN und Song dorthin kopieren (in 32-KB-Stücken)
            5. 32 KB konventionellen Speicher für den DMA-Puffer holen
            6. Boot-Information füllen, VGA-Modus 13h einschalten
            7. GDT laden, CR0.PE setzen, Fernsprung ──┐
                                                       v
          RTRACK.BIN (Protected Mode, 32 Bit)   entry.asm: Segmente, Stapel, BSS löschen,
                                                IDT, Interrupt-Controller auf 20h–2Fh
                                                       v
                                                rust_main (dos.rs): Optionen, Sound Blaster
                                                finden, Song laden, Wiedergabe starten,
                                                Hauptschleife bis Esc, Sound Blaster anhalten
                                                       v
          RTRACK.COM  <──  Fernsprung zurück, Real Mode, Interrupt-Controller und
                           Textmodus wiederherstellen, Meldungen ausgeben,
                           Speicher freigeben, Ende mit Rückgabewert
```

Die **Boot-Information** ist eine kleine Struktur im Lader, deren physische Adresse
`RTRACK.BIN` beim Start in einem Register bekommt: Ladeadresse, Speichergröße, Adresse und
Länge des Songs, Adresse des DMA-Puffers, Adressen der Werte von `BLASTER` und `RTRACK`, der
Meldungspuffer, die Kommandozeile und zwei Felder, in die der Einstieg bei einer
Prozessor-Ausnahme Adresse und Fehlercode schreibt.

## Speicher und Adressen

```
physisch
00000000  Interrupt-Tabelle des Real Mode, BIOS-Daten, DOS
          RTRACK.COM (Code, Daten, Meldungspuffer, 32-KB-Lesepuffer)
          DMA-Puffer (32 KB, davon 8 KB benutzt)
000A0000  VGA-Grafikspeicher (Modus 13h: 64 000 Bytes, ein Byte pro Pixel)
00100000  HMA (bleibt frei)
00110000  RTRACK.BIN, falls kein XMS (mit XMS: wo HIMEM.SYS den Block hinlegt)
            Code (~190 KB), Konstanten, Daten
            BSS (~8,9 MB): Zustand des Renderers mit Framebuffer, Mandelbrot- und
            Voxelpuffern, Paletten-Tabellen, Kanalpuffer, FFT
            Stapel (256 KB)
          Song (aufgerundet auf 4 KB)
```

Code- und Datensegment von `RTRACK.BIN` haben die Ladeadresse als Basis und eine Grenze von
4 GB. Eine physische Adresse `p` erreicht Rust deshalb über den Zeiger `p − Basis`: Für Adressen
unterhalb des Programms (VGA-Speicher, DMA-Puffer, Boot-Information) ist das eine riesige Zahl,
aber die Adressrechnung des Prozessors läuft modulo 2³² und landet genau bei `p`. So braucht
das Programm keine weiteren Segmente.

## Der Weg des Bildes: vom Rust-Array in den VGA-Speicher

1. **Rendern.** Die Hauptschleife ruft `vis_frame(now, 320, 200)`. Der Renderer zeichnet in
   sein Array `fb: [u32; FB_PIXELS]`, ein Feld im großen statischen Zustand `VIS` (also in der
   BSS des Abbilds): 320 × 200 Pixel im Format RGBA, je 32 Bit. Das ist exakt derselbe Code wie
   im Browser, wo dieses Array über JavaScript in ein Canvas kopiert wird. Zurück kommt ein
   Zeiger auf das Array.
2. **Palette wählen** (`update_palette`, alle vier Bilder). VGA zeigt in Modus 13h 256 Farben
   gleichzeitig, jede frei wählbar aus 262 144 (6 Bit pro Grundfarbe). Das Programm zählt, wie
   oft jede Farbe im Bild vorkommt, gerundet auf 5 Bit pro Grundfarbe (32 768 mögliche Farben).
   Die 255 häufigsten bekommen einen Palettenplatz (Platz 255 dient als Markierung, siehe
   unten). Das geschieht alle acht Bilder. Farben, die schon einen Platz haben, behalten
   ihn; nur neue Farben ändern Einträge. So verschieben sich die Farben bereits angezeigter
   Pixel nicht.
3. **Umrechnen** (`convert_frame`). Jedes RGBA-Pixel wird zu einem Byte, dem Palettenindex, im
   Array `BACK: [u8; 64000]`. Farben mit eigenem Platz bekommen ihn direkt; die übrigen den
   Platz mit der nächstliegenden Farbe. Das Ergebnis wird je Farbe in einer Tabelle mit einem
   Byte pro Farbe zwischengespeichert (32 KB, klein genug für den Cache des Pentium; 255 heißt
   „noch nicht gesucht“), bis sich die Palette ändert. Hat ein Pixel dieselbe Farbe wie sein
   linker Nachbar, was in Flächen, Schrift und verdoppelten Pixeln oft vorkommt, wird der Index
   einfach wiederholt. Dithering gibt es nicht mehr: Bei 256 passend gewählten Farben
   sind Verläufe sauber genug.
4. **Auf den Strahlrücklauf warten** (`wait_retrace`). Das Programm fragt Port 3DAh ab, bis der
   Elektronenstrahl unten angekommen ist und zurück nach oben läuft (Bit 3). In dieser Zeit ist
   ein Wechsel unsichtbar.
5. **Palette schreiben** (`write_dac`). Nur die geänderten Einträge: Index an Port 3C8h, dann
   Rot, Grün, Blau (je 6 Bit) an Port 3C9h.
6. **Kopieren** (`show_frame`). Die 64 000 Bytes aus `BACK` gehen in 16 000 Zugriffen zu je
   32 Bit an die physische Adresse A0000h. In Modus 13h ist der Grafikspeicher linear: Pixel
   (x, y) liegt bei A0000h + 320·y + x. Die Karte zeigt das ab dem nächsten Bildaufbau an.

Modus 13h hat nur eine Bildseite. Das Kopieren dauert länger als der Strahlrücklauf, deshalb kann
im unteren Bildteil kurz eine Kante zu sehen sein. Der „Mode X“ der VGA hätte mehrere Seiten zum
Umschalten, verlangt aber ein umständlicheres Schreiben in vier Ebenen.

## Der Weg des Klangs: vom Mischer über DMA zum Sound Blaster

Der Sound Blaster holt sich die Klangdaten selbst aus dem Speicher, über den DMA-Controller des
PC (zwei 8237-Bausteine). Der Prozessor muss nur dafür sorgen, dass rechtzeitig neue Daten
bereitliegen.

```
mp_render (Replayer, Rust)       DMA-Puffer im konventionellen Speicher
  1024 Stereo-Frames f32  ──>   ┌──────────────┬──────────────┐
  in i16 umgewandelt             │  Hälfte 0    │  Hälfte 1    │  je 1024 Frames
  (im Interrupt)                 └──────────────┴──────────────┘
                                   ^ DMA-Controller liest im Kreis (Auto-Init)
                                   v
                                 Sound Blaster (DSP) ──> Lautsprecher
                                   │ nach jeder Hälfte: Interrupt (IRQ 5)
                                   v
                                 rust_irq: Hälfte, die gerade fertig wurde, neu füllen
```

* **Der Puffer** liegt im konventionellen Speicher, weil der DMA-Controller des ISA-Busses nur
  die ersten 16 MB physischen Speichers erreicht und ein Transfer keine 64-KB-Grenze
  (16-Bit-DMA: 128-KB-Grenze) überschreiten darf. Der Lader holt dafür 32 KB von DOS, `dos.rs`
  sucht darin eine passende Lage für 8 KB.
* **Die Einstellungen der Karte** (Port, IRQ, DMA-Kanäle) stehen in der Umgebungsvariablen
  `BLASTER`, die die Installation der Karte in die AUTOEXEC.BAT schreibt, z. B.
  `A220 I5 D1 H5 T6`. Fehlt sie, gelten diese Werte. Beim Sound Blaster 16 liest das Programm
  IRQ und DMA außerdem direkt aus den Mixer-Registern 80h und 81h der Karte und nimmt diese
  Werte, wenn sie von `BLASTER` abweichen; es meldet das beim Beenden. Ist kein 16-Bit-DMA-Kanal
  eingestellt, spielt es wie ein Sound Blaster Pro mit 8 Bit.
* **Sound Blaster 16** (DSP-Version 4 oder höher): 44,1 kHz, 16 Bit, Stereo über den 16-Bit-
  DMA-Kanal (meist 5). Programmiert werden Adresse, Seite und Länge des ganzen Puffers im
  DMA-Controller (Ports C4h–D8h, Seitenregister 8Bh), Modus „Auto-Init, Lesen“; dann am DSP die
  Abtastrate (Befehl 41h) und die Wiedergabe mit Befehl B6h (16 Bit, Auto-Init, FIFO), Modus 30h
  (Stereo, vorzeichenbehaftet) und der Länge einer Hälfte. Nach jeder Hälfte meldet sich die
  Karte mit einem Interrupt, und der DMA-Controller liest ohne Pause in der anderen Hälfte weiter.
* **Sound Blaster 2.0/Pro**: 22 kHz, 8 Bit, Mono über den 8-Bit-DMA-Kanal (meist 1), mit
  Zeitkonstante (Befehl 40h), Blocklänge (48h) und Auto-Init-Wiedergabe (1Ch).
* **Der Interrupt.** Der Einstieg in `entry.asm` rettet die Register und den FPU-Zustand, denn
  das Hauptprogramm könnte gerade mitten in einer Gleitkomma-Rechnung des Renderers sein, und
  der Mischer rechnet ebenfalls in Gleitkomma. Dann ruft er `rust_irq` auf. Diese Funktion
  bestätigt den Interrupt am DSP (Lesen von Port 22Fh bzw. 22Eh), mischt mit `mp_render` die
  nächsten 1024 Frames in die Hälfte, die gerade abgespielt wurde, und schickt dem
  Interrupt-Controller das Ende des Interrupts (EOI). Weil die Musik so im Interrupt entsteht,
  läuft sie gleichmäßig weiter, egal wie lange ein Bild zum Rendern braucht.
* **Daten für die Anzeige.** Im selben Interrupt kopiert `capture` die Kanalsignale und das
  Summensignal des Blocks in einen Zwischenpuffer. Die Hauptschleife übernimmt sie in den
  Renderer, wie im Browser die Nachrichten des AudioWorklets. Das Spektrum (eine FFT über 4096
  Werte) berechnet sie nur für die Ansichten, die es zeigen (Spektrum, Würfel, See), und dort
  jedes zweite Mal; auf einem Pentium 133 kostet es spürbar Zeit.
* **Die Uhr der Anzeige.** Welcher Moment gerade zu hören ist, ergibt sich aus der Zahl der
  Interrupts (fertige Hälften) und dem Stand des DMA-Zählers. Danach richten sich Oszilloskop,
  Pattern-Zeile und alle Effekte.

Stimmen IRQ oder DMA-Kanal nicht, kommt nie ein Interrupt: Es entsteht kein Ton, und weil die
Uhr der Anzeige an den Interrupts hängt, steht auch das Bild still. Das Programm erkennt das
nach 100 Bildern ohne Interrupt (mindestens anderthalb Sekunden, weil jedes Bild auf einen
Strahlrücklauf wartet), beendet sich und erklärt, was zu prüfen ist. Plug-and-Play-Karten wie
der Sound Blaster 16 PnP sind unter reinem DOS erst nutzbar, wenn ihr Treiber (bei Creative
CTCM) sie eingerichtet hat; Windows 98 erledigt das, auch für den „MS-DOS-Modus“.

Beim Beenden hält das Programm die Wiedergabe an (DSP-Befehle D5h/D9h bzw. D0h/DAh), maskiert
den Interrupt, sperrt den DMA-Kanal und setzt den DSP zurück.

## Tastatur

Der Tastatur-Interrupt bleibt gesperrt; das Programm liest die Scancodes in der Hauptschleife
direkt vom Tastatur-Controller (Status an Port 64h, Daten an Port 60h). Ein eigener
Interrupt-Handler wäre aufwendiger und bringt hier nichts, weil die Hauptschleife ohnehin
ständig läuft.

## Kommandozeile

```
RTRACK [options] [FILE.MOD]
  /M  add the Mandelbrot zoom (slow)
  /W  add the cube (slow)
  /V  add the voxel landscape (very slow)
  /?  this help
```

Mandelbrot-Zoom und Voxel-Landschaft brauchen auf einem Pentium und in DOSBox mehrere Sekunden
pro Bild, der Würfel mit seinen sechs lebenden Flächen ruckelt auf einem Pentium 133 stark.
Ohne Option springt die Taste V deshalb über sie hinweg. Der Lader überspringt Wörter,
die mit `/` oder `-` beginnen, bei der Suche nach dem Songnamen; `RTRACK.BIN` liest die Optionen
selbst aus der Kommandozeile im PSP und gibt dem Renderer mit `vis_set_views` die erlaubten
Ansichten vor.

## Testen

* **Echte Hardware.** Ein Pentium 133 mit 96 MB RAM, VGA und Sound Blaster 16, unter MS-DOS 5.0
  und im MS-DOS-Modus von Windows 98. Alle Ansichten laufen. Oszilloskop und 2D-Spektrum ruckeln
  leicht, der See braucht beim ersten Aufruf ein paar Sekunden für den Aufbau, die
  Demoszene-Effekte laufen, die Laufschrift aber weniger flüssig als im Emulator; Mandelbrot,
  Würfel und Voxel sind langsam und deshalb nur per Option dabei. Die Tests auf diesem Rechner haben Fehler gefunden, die kein Emulator
  gezeigt hat:
  * Unter DOS 5.0 stimmte die `BLASTER`-Einstellung nicht mit der Karte überein: kein Ton und
    ein stehendes Bild. Seitdem liest das Programm IRQ und DMA beim SB16 aus der Karte und
    meldet fehlende Interrupts.
  * Das 3D-Spektrum war zu langsam, um sich sichtbar zu bewegen, und die Mandelbrot-Fläche des
    Würfels wurde nie fertig. Seitdem Festkomma-Linien und kleinere Würfelflächen.
  * Der erste Aufbau des Sees dauerte so lange, dass das Programm eingefroren wirkte. Seitdem
    werden die Uferfunktionen nur einmal pro Zelle berechnet, und ein Wachhund meldet, falls die
    Hauptschleife wirklich hängt.
* **Linux-Testrahmen** (`dos/selftest.asm`, `dos/selftest_check.py`). Derselbe Rust-Code läuft
  als 32-Bit-Linux-Programm: Der Interrupt wird simuliert, Port-Zugriffe tun nichts, die
  Ladeadresse ist 0. Das prüft Mischer, Renderer, Palette und Umrechnung und liefert für jede
  Ansicht ein Bild. Mit Valgrind (`--tool=callgrind`) lässt sich hier auch messen, wo die
  Rechenzeit bleibt.
* **DOSBox in der CI** (`dos/dosbox_test.sh`). Der vollständige Weg mit Lader, Protected Mode,
  Sound Blaster 16 und VGA: einmal 8 Sekunden mit dem schnellen, einmal 30 Sekunden mit dem
  genauen CPU-Kern, mit Screenshots und der Ausgabe von `RTRACK.COM`. Die Umgebungsvariable
  `RTRACK=T8` lässt das Programm nach 8 Sekunden Wiedergabe von selbst enden.
* **js-dos** (DOSBox im Browser) haben wir ausprobiert. Dessen Emulator stürzte in unseren Tests
  schon an der DOS-Eingabeaufforderung ab, unabhängig vom Rustracker. Die DOS-Seite zeigt
  deshalb die Bilder aus der CI.

## Grenzen

* Real-Mode-DOS ohne EMM386, QEMM oder Windows (siehe oben).
* Etwa 10 MB Erweiterungsspeicher, also ein Rechner mit 16 MB.
* Ohne HIMEM.SYS geht der Lader davon aus, dass der Speicher ab 1,1 MB frei ist, und erreicht
  über das BIOS nur die ersten 16 MB.
* Der Timer-Interrupt ist während der Wiedergabe gesperrt; die DOS-Uhr bleibt in dieser Zeit
  stehen.
* Mandelbrot, Würfel, Voxel und See sind für einen Pentium 133 zu aufwendig; sie laufen, aber
  langsam.
