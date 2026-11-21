# Rust MOD Tracker (WebAssembly) mit Oszilloskop

[![CI](https://github.com/MarkusFlad/rustracker/actions/workflows/ci.yml/badge.svg)](https://github.com/MarkusFlad/rustracker/actions/workflows/ci.yml)

ProTracker-MOD-Player, geschrieben in Rust ohne externe Crates. Er wird zu WebAssembly
kompiliert und spielt im Browser über ein AudioWorklet ab. Dazu gibt es ein Oszilloskop
(Master L/R, XY-Lissajous und eine Spur pro Kanal) und eine Pattern-Ansicht wie in einem Tracker.

**Live-Demos** (von der CI bei jedem Push gebaut und auf GitHub Pages veröffentlicht):

- [▶ Klassische Edition](https://markusflad.github.io/rustracker/): Oberfläche in HTML/Canvas, Mandelbrot auf der GPU (WebGL2) oder mit dem Rust-SIMD-Kern
- [▶ Framebuffer-Edition](https://markusflad.github.io/rustracker/fb/): jedes Pixel von Rust gezeichnet, nur ein paar Zeilen JavaScript

## Starten

Ausprobieren geht am schnellsten über die Live-Demos oben. Lokal:

```sh
./build.sh          # baut die WASM-Dateien und startet http://localhost:8080
```

Die WASM-Dateien (`modplayer.wasm`, `mandel.wasm`) liegen nicht im Repository: `.gitignore`
hält `*.wasm` und `*.wav` heraus, und die CI baut sie bei jedem Push selbst. `build.sh` erzeugt
sie lokal und kopiert sie nach `www/` und `www-fb/`. Danach startet es den Webserver für die
klassische Edition. Für die Framebuffer-Edition:

```sh
./build.sh --no-serve && cd www-fb && python3 -m http.server 8080
```

Voraussetzung für den Build: rustup/cargo und Python 3 (für den lokalen Webserver). Das Skript
installiert das Target `wasm32-unknown-unknown`, falls es fehlt. wasm-bindgen oder wasm-pack
werden nicht gebraucht. Wer nur die fertigen Dateien will, findet sie auch im Artefakt `site`
jedes CI-Laufs (Tab *Actions*).

Im Browser auf **Play** tippen. Standard-Song ist „Neon Techno“ von Drozerix (`www/drozerix_-_neon_techno.mod`, Public Domain, siehe [CREDITS.md](CREDITS.md)). Daneben liegen die eigenen Stücke „Orbitalfunk“ und „November Static“. Einen anderen Standard-Song bettest du mit `python3 scripts/embed_song.py <datei.mod>` in `www/song.js` ein. Fremde Module bitte nicht committen; `.gitignore` hält zum Beispiel `www/12th_warrior.mod` heraus. Eigene `.mod`-Dateien lassen sich über **Datei …** oder per
Drag & Drop laden. Wenn du auf eine Position in der Order-Liste tippst, springt der Player dorthin.

### Ohne wasm32-Target bauen

Die ganze Bibliothek (Replayer, FFT, Renderer, API) verwendet nur den Sprachkern: keine
Iteratoren, keine `Option`, keine Slices, keine Allokation, keine Methoden aus `core`. Deshalb
lässt sie sich auch ohne installiertes wasm32-Target mit einem winzigen Ersatz für `core`
direkt nach WebAssembly übersetzen. Die CI baut diese Variante zur Kontrolle bei jedem Push mit:

```sh
mkdir -p preview/out
RUSTC_BOOTSTRAP=1 rustc --edition 2021 --target wasm32-unknown-unknown --crate-type cdylib \
  -C opt-level=3 -C panic=abort -C overflow-checks=no -C strip=symbols \
  -o preview/out/modplayer.wasm preview/minicore.rs
cp preview/out/modplayer.wasm www/ && cp preview/out/modplayer.wasm www-fb/
```

Die so gebaute WASM-Datei gibt den Song bis auf ±1 LSB identisch mit dem nativen Build aus.

## Nativ testen

```sh
cargo test
cargo run --release --bin mod2wav -- www/november_static.mod song.wav
cargo run --release --bin fbshot -- www/drozerix_-_neon_techno.mod /tmp/frames 1280 800
```

## Dokumentation

Jede öffentliche Funktion ist mit Zweck, Argumenten und Rückgabewert dokumentiert (die CI
bricht ab, wenn etwas fehlt). Als HTML ansehen:

```sh
cargo doc --no-deps --open
```

## Aufbau

| Datei | Inhalt |
|---|---|
| `src/replayer.rs` | MOD-Parser, Effekte, Mixer (lineare Interpolation, LRRL-Panning, Lautstärkerampen) |
| `src/fft.rs` | wiederverwendbare FFT (Radix-2, Hann-Fenster) mit linearem Puffer oder Ringpuffer als Eingang |
| `src/spectrum.rs` | Auswertung eines Leistungsspektrums: Bänder, Spitze mit Parabel-Interpolation, Log-Frequenzachse, dB-Skala |
| `src/math.rs` | Mathematik ohne `core`: sin/cos, log2/log10, exp2, pow, sqrt, Begrenzungen |
| `src/api.rs` | schlanke C-ABI-Exporte (`mp_load`, `mp_render`, `mp_channels_ptr`, `vis_*`, …) |
| `src/lib.rs` | bindet alles ein; auf wasm32 `no_std`, `#![warn(missing_docs)]` |
| `preview/minicore.rs` | Mini-Ersatz für `core`, für Builds ohne wasm32-Target |
| `src/bin/mod2wav.rs` | Werkzeug zum nativen Rendern in eine WAV-Datei |
| `www/worklet.js` | AudioWorklet: ruft `mp_render` auf und schickt Scope-Daten an die Oberfläche |
| `www/index.html` | Oberfläche, Oszilloskop (Canvas mit Phosphor-Nachleuchten), Pattern-Ansicht, Musik-Regie für den Mandelbrot-Zoom |
| `www/mandel.js` | Mandelbrot-Engine aus dem Echtzeit-Zoomer: GPU (WebGL2, fp32/df64) und der Rust-SIMD-Kern in Web Workern |
| `www/voxel.js` | Voxel-Landschaft aus dem Mandelbrot-Zoom (Höhenkarte vom SIMD-Kern) und der gemeinsame Voxel-Space-Renderer |
| `www/lake.js` | See aus Voxeln: Wellengleichung, angeregt von Bass und Takt |
| `kernel/mandel_wasm.rs` | der SIMD-Kern des Zoomers (f64x2, 8 Pixel verschränkt), gebaut zu `mandel.wasm`; liefert Farben oder Palettenindizes |

Unterstützt werden 15/31-Sample-MODs mit 4 bis 32 Kanälen (M.K., FLT4/8, xCHN, xxCH)
und die ProTracker-Effekte 0–F. Dazu gehören auch die E-Befehle: Fine Slides, Loop,
Retrigger, Note Cut/Delay und Pattern Delay.

## Mandelbrot-Ansicht

Über **Ansicht** wechselst du vom Oszilloskop zum Mandelbrot-Zoom. Das Timing kommt direkt
aus dem Replayer: Position, Zeile, Speed und BPM. Ein Schlag sind 4 Zeilen, ein Takt 16 Zeilen.

- Jede Order-Position startet eine neue Fahrt zu einem anderen Ort im Apfelmännchen, mit einem Lichtblitz auf der Eins.
- Jeder Schlag gibt einen Zoom-Schub. Je lauter die Musik, desto schneller geht es hinein.
- Der Bildmittelpunkt wandert pro Schlag eine Vierteldrehung im Kreis, und pro Takt schwingt das Bild nach links oder rechts.
- Jede gespielte Note verschiebt die Farbpalette.

Die Anzeige läuft um die Audio-Ausgabelatenz verzögert, damit Bild und Ton zusammenpassen.
Doppelklick oder **Vollbild** füllt den Bildschirm.

## Spektrum-Ansicht

Die dritte Ansicht zeigt das Frequenzspektrum der Musik. Die API (`src/api.rs`) hält dafür die
letzten 4096 Samples des Mixes (Mono) in einem Ringpuffer. `mp_spectrum(4096)` berechnet daraus
mit einer eigenen FFT (Radix-2, Cooley-Tukey, Hann-Fenster) das Leistungsspektrum. Das geschieht
alle 1024 Samples, also mit 75 % Überlappung. Die FFT kommt ohne `core`-Mathematik aus: Sinus und
Kosinus für die Twiddle-Faktoren kommen per Taylor-Reihe aus `src/math.rs`. Getestet ist sie gegen eine naive
DFT (`cargo test`).

Angezeigt werden:
- Balken auf logarithmischer Frequenzachse (30 Hz bis 20 kHz) mit Spitzenwert-Markern, Skala in dB.
- Ein mitlaufendes Spektrogramm darunter.
- Die stärkste Frequenz mit Notenname.

Mit **Darstellung: 3D** wird das Spektrum in die Tiefe erweitert. Vorne steht das aktuelle
Spektrum, dahinter liegen die Spektren der letzten 4 Sekunden. Sie rücken perspektivisch nach
hinten, werden kleiner und blasser und verdecken einander (Maleralgorithmus). Was älter als
4 Sekunden ist, wird nicht mehr gezeichnet. In lauten Passagen färben sich die Linien von Grün
nach Bernstein.

## Songs

Die beiden eigenen Songs werden von Python-Skripten in `songs/` erzeugt. Sie synthetisieren alle Samples
selbst und schreiben die MOD-Datei. Dafür werden Python 3 und numpy gebraucht.

- `songs/make_space.py` → „Orbitalfunk“: 8 Kanäle (8CHN), D-Moll, 124 BPM. Lead mit Echo-Kanal, Arpeggio, Pad mit eingebautem Einschwingen, Oktav-Bass, Drums mit Gated-Snare.
- `songs/make_mod.py` → „November Static“: 4 Kanäle (M.K.).

## Würfel-Ansicht

Die vierte Ansicht zeigt einen langsam taumelnden 3D-Würfel. Er ist mit CSS-3D gebaut, also
perspektivisch korrekt und mit scharfer Schrift. Jede der sechs Seiten ist eine eigene Canvas,
die jedes Bild neu gezeichnet wird:

- Master L/R
- XY
- Spektrum 2D
- Spektrum 3D mit 4 s Verlauf
- Tracker-Noten des aktuellen Patterns
- Mandelbrot-Zoom, eine eigene GPU-Instanz mit derselben Musik-Regie

Der Würfel dreht sich um zwei Achsen, sodass nach und nach jede Seite nach vorne kommt. Auf
jedem Schlag pulsiert er leicht, auf der Eins jedes Takts etwas stärker.

## Voxel-Ansicht

Die fünfte Ansicht macht aus dem Mandelbrot-Zoom eine Landschaft aus Voxeln. Grundlage ist
dasselbe Bild der Musik-Regie, als quadratische Karte der Iterationszahlen:

- Die Mandelbrotmenge selbst liegt auf Höhe null.
- Je weniger Iterationen ein Punkt braucht, desto höher liegt er. Die Höhen sind über das Bild
  histogramm-normiert: Die Höhe ist der Anteil der Punkte, die mehr Iterationen brauchen. So
  verteilen sie sich gleichmäßig, und die Bereiche mit den wenigsten Iterationen bilden die Gipfel.
- Jedes Voxel hat die Farbe des 2D-Bildes, inklusive Palettenverschiebung und Lichtblitz, und
  wird nach seiner Neigung beleuchtet. In der Ferne verschwimmt es im Dunst.
- Die Kamera umkreist die Landschaft einmal pro Minute und schaut dabei links an der Mitte vorbei,
  fliegt also um die Gipfel herum. Auf jedem Schlag sinkt sie ein kleines Stück ab.
- Der Zoom der Musik-Regie lässt die Landschaft wachsen. Ein neues Bild ersetzt die Karte erst,
  wenn es fertig ist, dazwischen wird das alte passend zum aktuellen Zoom umgerechnet.

Gezeichnet wird mit dem klassischen Voxel-Space-Verfahren: ein Strahl pro Bildspalte, von vorn
nach hinten über die Höhenkarte, jede Stelle als senkrechter Streifen über dem, was schon näher
liegt. In der Framebuffer-Edition steckt das in `src/vis_voxel.rs` (die Karte rechnet der
SIMD-Kern in den Workern), in der klassischen Edition in `www/voxel.js` (eigener Worker-Pool mit
dem SIMD-Kern, gerendert in reduzierter Auflösung).

## See-Ansicht

Die sechste Ansicht zeigt einen See aus Voxeln, den die Musik zum Schwingen bringt:

- Die Wasseroberfläche ist eine zweidimensionale, leicht gedämpfte Wellengleichung auf einem
  Gitter (240 × 240 Zellen, 120 Schritte pro Sekunde). Wellenberge und -täler addieren und
  löschen sich deshalb wie bei echten Wellen, und am Ufer werden sie zurückgeworfen.
- Jeder Einsatz des tiefsten Basses (etwa 30 bis 75 Hz, der Pegel springt über seinen
  gleitenden Mittelwert) wirft einen „Stein“ ins Wasser, je stärker der Sprung, desto tiefer
  die Delle. Schläge ohne Bass-Einsatz werfen einen kleineren. Von dort läuft eine konzentrische
  Welle los.
- Die Stelle, an der die Steine fallen, wandert langsam im Kreis um die Mitte des Sees.
- Das Ufer ist unregelmäßig geformt (ein Kreis, verformt durch einige Oberschwingungen mit
  festen Phasen), und in der Nähe der Mitte liegt eine kleine Insel. An beiden werden die Wellen
  zurückgeworfen.
- Das Wasser ist türkis, auf den Wellenbergen heller und mit Schaum auf hohen Kämmen, und wird
  nach seiner Neigung beleuchtet. Das Ufer steigt zu Hügeln an: Sand, Gras, weiter oben Fels.
- Die Kamera fliegt langsam: etwa eine Minute für eine Runde um den See, in etwa anderthalb
  Minuten einmal weg und wieder heran, in etwa einer Minute einmal hoch und wieder herab. Der
  Blick folgt dabei so, dass der See im Bild bleibt.

**Auflösung:** In beiden Voxel-Ansichten schaltet der Knopf **Grob/Fein** (Taste R, in der
klassischen Edition „Auflösung“) zwischen einer glatten Oberfläche (Standard) und den kantigen Voxeln um.
Fein interpoliert Höhen und Farben bilinear zwischen den Zellen und tastet die Strahlen enger ab.
Beim See entscheidet der feine Modus außerdem pro Bildpunkt anhand des interpolierten Abstands zur
Uferlinie, ob Wasser, Strand oder Wiese zu sehen ist; das Ufer wird so zur glatten Kurve statt
der Treppe der Gitterzellen;
die klassische Edition rechnet dann außerdem in voller Bildschirmauflösung statt in reduzierter.
Das kostet etwa die doppelte bis dreifache Rechenzeit.

Beide Voxel-Ansichten nutzen denselben Renderer: `src/voxel_space.rs` in der
Framebuffer-Edition, `VoxelSpace` in `www/voxel.js` in der klassischen.

## Continuous Integration

`.github/workflows/ci.yml` läuft bei jedem Push und Pull Request:

| Job | Prüft |
|---|---|
| **Rust** | `cargo fmt --check`, `cargo clippy -D warnings`, `cargo doc` (jedes öffentliche Element dokumentiert, keine kaputten Links), `cargo test`, natives Rendern der drei Songs |
| **WebAssembly** | Build mit `cargo` für `wasm32-unknown-unknown` **und** über `preview/minicore.rs`. Der zweite Build stellt sicher, dass die Bibliothek weiterhin nur den Sprachkern nutzt. Dazu kommt der Mandelbrot-SIMD-Kern (`mandel.wasm`). Danach laufen in Node `tests/wasm_smoke.mjs` (Exporte, Laden, Klang, FFT, Vergleich mit dem nativen Rendern auf ±2 LSB) und `tests/vis_smoke.mjs` (Framebuffer-Renderer, mit und ohne SIMD-Kern). Zum Schluss werden die Skripte beider Seiten geprüft und die Website als Artefakt `site` gebaut. |
| **Songs** | Erzeugt die beiden eigenen Songs neu aus `songs/*.py` und prüft, dass der Rust-Player sie abspielt |
| **Pages** | Nur bei Push auf den Standard-Branch (`main` oder `master`): veröffentlicht beide Editionen mit „Neon Techno“ auf GitHub Pages (klassisch unter `/`, Framebuffer unter `/fb/`) |

Für GitHub Pages musst du einmalig unter *Settings → Pages → Source* „GitHub Actions“ wählen.
Danach öffnet [https://markusflad.github.io/rustracker/](https://markusflad.github.io/rustracker/) direkt den Tracker und [https://markusflad.github.io/rustracker/fb/](https://markusflad.github.io/rustracker/fb/) die Framebuffer-Edition; die WASM-Dateien dafür baut die CI bei jedem Push neu.

Lokal laufen dieselben Prüfungen mit:

```sh
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
RUSTFLAGS="-D missing_docs" RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
./build.sh --no-serve
cargo run --release --bin mod2wav -- www/drozerix_-_neon_techno.mod /tmp/native.wav 10
node tests/wasm_smoke.mjs www/modplayer.wasm www/drozerix_-_neon_techno.mod /tmp/native.wav
node tests/vis_smoke.mjs www-fb/modplayer.wasm www/drozerix_-_neon_techno.mod www-fb/mandel.wasm
```

## Framebuffer-Edition (`www-fb/`)

Eine zweite Oberfläche, bei der **jedes Pixel in Rust gezeichnet wird**: Knöpfe, Schrift,
Oszilloskop, Spektrum 2D/3D, Mandelbrot-Zoom, Würfel, Voxel-Landschaft, See und Pattern-Ansicht. Rust rendert in einen
RGBA-Framebuffer im WASM-Speicher, JavaScript legt ihn per `putImageData` auf ein Canvas.
Das JavaScript (gut 100 Zeilen samt dem kleinen Worker für den Mandelbrot-Kern) macht nur noch Folgendes:

- WASM laden, AudioWorklet starten und dessen Daten in einen Puffer des Renderers kopieren
- jedes Bild `vis_frame()` aufrufen und den Framebuffer anzeigen
- Klicks und Tasten an Rust weiterreichen (`vis_pointer`, `vis_key`) und die zurückgegebene
  Aktion ausführen (Play, Stop, Datei öffnen, Vollbild, Springen, Loop)

| Datei | Inhalt |
|---|---|
| `src/vis.rs` | Zustand des Renderers, Datenübernahme (Pegel, Bänder, Spektrogramm, 3D-Verlauf), Layout, Ansichtswahl |
| `src/vis_osc.rs` | Oszilloskop: Master L/R, XY, Kanäle |
| `src/vis_mandel.rs` | Musik-Regie, Mandelbrot (Job-Protokoll für die Worker oder eigener Pfad, Reprojektion) |
| `src/vis_spec.rs` | Spektrum 2D mit Spektrogramm und 3D-Verlauf mit Floating Horizon |
| `src/vis_cube.rs` | Würfel mit sechs Live-Flächen, perspektivisch korrekte Texturen |
| `src/vis_voxel.rs` | Voxel-Landschaft aus dem Mandelbrot-Zoom mit Kameraflug |
| `src/vis_lake.rs` | See aus Voxeln: Wellengleichung, angeregt von Bass und Takt |
| `src/voxel_space.rs` | wiederverwendbarer Voxel-Space-Renderer für Höhenkarten (Kamera-Orbit, Strahlen pro Spalte, Dunst) |
| `src/vis_ui.rs` | Knopfleiste, Eingabe, Seitenleiste mit Order-Liste und Pattern |
| `src/gfx.rs` | Framebuffer-Primitive: Rechtecke, Linien mit Glow, Text |
| `src/font.rs` | eigener 5×7-Bitmap-Font (erzeugt von `tools/make_font.py`) |
| `src/bin/fbshot.rs` | rendert Frames nativ als PPM-Bilder, zum Testen ohne Browser |

Tasten: Leertaste Play/Pause, S Stop, O Datei, V Ansicht, 3 Spektrum 2D/3D, P Farben, R Voxel grob/fein, L Loop, F Vollbild.

Wie alles andere bleibt auch der Renderer im Sprachkern-Subset und baut deshalb auch über
`preview/minicore.rs`. `tests/vis_smoke.mjs` prüft ihn in Node (alle Ansichten, Knöpfe, Zeit pro Frame).
Auf GitHub Pages liegt die Framebuffer-Edition unter [/fb/](https://markusflad.github.io/rustracker/fb/).

### Mandelbrot-Kern

`kernel/mandel_wasm.rs` ist der Rechenkern aus dem Echtzeit-Zoomer: WebAssembly-SIMD mit
2 doubles pro Vektor, 8 Pixel verschränkt, jede Lane zählt ihre Iterationen über eine Maske.
Beide Oberflächen laden ihn als `mandel.wasm` in Web Worker. Die Framebuffer-Edition lässt
sich davon Palettenindizes liefern: Rust vergibt die Aufträge (`vis_mb_take`, `vis_mb_param`),
die Worker rechnen Zeilenblöcke, das JavaScript kopiert sie zurück (`vis_mb_rows`). Ohne Worker
oder ohne SIMD rechnet der Renderer selbst, mit dem einfacheren Pfad in `src/vis_mandel.rs`.

`build.sh` baut ihn mit und kopiert ihn in beide Oberflächen; von Hand:

```sh
RUSTC_BOOTSTRAP=1 rustc --edition 2021 --target wasm32-unknown-unknown --crate-type cdylib \
  -C opt-level=3 -C target-feature=+simd128 -C panic=abort -C strip=symbols \
  -o www/mandel.wasm kernel/mandel_wasm.rs
cp www/mandel.wasm www-fb/
```
