# Rust MOD Tracker (WebAssembly) mit Oszilloskop

[![CI](https://github.com/MarkusFlad/rustracker/actions/workflows/ci.yml/badge.svg)](https://github.com/MarkusFlad/rustracker/actions/workflows/ci.yml)

ProTracker-MOD-Player, geschrieben in Rust ohne externe Crates. Er wird zu WebAssembly
kompiliert und spielt im Browser über ein AudioWorklet ab. Dazu gibt es ein Oszilloskop
(Master L/R, XY-Lissajous und eine Spur pro Kanal) und eine Pattern-Ansicht wie in einem Tracker.

**Live-Demos** (von der CI bei jedem Push gebaut und auf GitHub Pages veröffentlicht):

- [▶ Klassische Edition](https://markusflad.github.io/rustracker/): Oberfläche in HTML/Canvas, Mandelbrot auf der GPU (WebGL2) oder mit dem Rust-SIMD-Kern
- [▶ Framebuffer-Edition](https://markusflad.github.io/rustracker/fb/): jedes Pixel von Rust gezeichnet, nur ein paar Zeilen JavaScript

## Starten

```sh
./build.sh          # baut modplayer.wasm und startet http://localhost:8080
```

Eine fertige `www/modplayer.wasm` liegt schon bei. Wenn du sie nur starten willst:
`cd www && python3 -m http.server 8080`.

Voraussetzung für den Build: rustup/cargo und Python 3 (für den lokalen Webserver). Das Skript installiert
das Target `wasm32-unknown-unknown`, falls es fehlt. wasm-bindgen oder wasm-pack werden
nicht gebraucht.

Im Browser auf **Play** tippen. Standard-Song ist „Neon Techno“ von Drozerix (`www/drozerix_-_neon_techno.mod`, Public Domain, siehe [CREDITS.md](CREDITS.md)). Daneben liegen die eigenen Stücke „Orbitalfunk“ und „November Static“. Einen anderen Standard-Song bettest du mit `python3 scripts/embed_song.py <datei.mod>` in `www/song.js` ein. Fremde Module bitte nicht committen; `.gitignore` hält `www/12th_warrior.mod` heraus. Eigene `.mod`-Dateien lassen sich über **Datei …** oder per
Drag & Drop laden. Wenn du auf eine Position in der Order-Liste tippst, springt der Player dorthin.

### Ohne wasm32-Target bauen

`src/replayer.rs` und `src/api.rs` verwenden nur den Sprachkern: keine Iteratoren,
keine `Option`, keine Slices, keine Allokation. Deshalb lassen sie sich auch mit einem
winzigen Ersatz für `core` direkt nach WebAssembly übersetzen (so wurde die mitgelieferte
`www/modplayer.wasm` gebaut):

```sh
RUSTC_BOOTSTRAP=1 rustc --edition 2021 --target wasm32-unknown-unknown --crate-type cdylib \
  -C opt-level=3 -C panic=abort -C overflow-checks=no -C strip=symbols \
  -o www/modplayer.wasm preview/minicore.rs
```

Die so gebaute WASM-Datei gibt den Song bis auf ±1 LSB identisch mit dem nativen Build aus.

## Nativ testen

```sh
cargo test
cargo run --release --bin mod2wav -- www/november_static.mod song.wav
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

Die dritte Ansicht zeigt das Frequenzspektrum der Musik. Der Rust-Replayer hält dafür die
letzten 4096 Samples des Mixes (Mono) in einem Ringpuffer. `mp_spectrum(4096)` berechnet daraus
mit einer eigenen FFT (Radix-2, Cooley-Tukey, Hann-Fenster) das Leistungsspektrum. Das geschieht
alle 1024 Samples, also mit 75 % Überlappung. Die FFT kommt ohne `core`-Mathematik aus: Sinus und
Kosinus für die Twiddle-Faktoren rechnet sie per Taylor-Reihe. Getestet ist sie gegen eine naive
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

Die beiden Songs werden von Python-Skripten in `songs/` erzeugt. Sie synthetisieren alle Samples
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

## Continuous Integration

`.github/workflows/ci.yml` läuft bei jedem Push und Pull Request:

| Job | Prüft |
|---|---|
| **Rust** | `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, natives Rendern beider Songs |
| **WebAssembly** | Build mit `cargo` für `wasm32-unknown-unknown` **und** über `preview/minicore.rs`. Der zweite Build stellt sicher, dass Replayer, FFT und API weiterhin nur den Sprachkern nutzen. Danach läuft `tests/wasm_smoke.mjs` in Node: Exporte, Laden, Klang, FFT und Vergleich mit dem nativen Rendern (±2 LSB). Zum Schluss werden die Skripte der Seite geprüft und die Seite als Artefakt gebaut. |
| **Songs** | Erzeugt beide Songs neu aus `songs/*.py` und prüft, dass der Rust-Player sie abspielt |
| **Pages** | Nur bei Push auf den Standard-Branch (`main` oder `master`): veröffentlicht die Demo-Seite mit „Neon Techno“ auf GitHub Pages |

Für GitHub Pages musst du einmalig unter *Settings → Pages → Source* „GitHub Actions“ wählen.
Danach öffnet [https://markusflad.github.io/rustracker/](https://markusflad.github.io/rustracker/) direkt den Tracker und [https://markusflad.github.io/rustracker/fb/](https://markusflad.github.io/rustracker/fb/) die Framebuffer-Edition; die WASM-Datei dafür baut die CI bei jedem Push neu.
Im Badge oben `OWNER/REPO` durch den echten Repo-Pfad ersetzen.

Lokal laufen dieselben Prüfungen mit:

```sh
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
cargo build --release --target wasm32-unknown-unknown --lib
cargo run --release --bin mod2wav -- www/drozerix_-_neon_techno.mod /tmp/native.wav 10
node tests/wasm_smoke.mjs target/wasm32-unknown-unknown/release/modplayer.wasm www/drozerix_-_neon_techno.mod /tmp/native.wav
```

## Framebuffer-Edition (`www-fb/`)

Eine zweite Oberfläche, bei der **jedes Pixel in Rust gezeichnet wird**: Knöpfe, Schrift,
Oszilloskop, Spektrum 2D/3D, Mandelbrot-Zoom, Würfel und Pattern-Ansicht. Rust rendert in einen
RGBA-Framebuffer im WASM-Speicher, JavaScript legt ihn per `putImageData` auf ein Canvas.
Das JavaScript (~80 Zeilen) macht nur noch Folgendes:

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
| `src/vis_ui.rs` | Knopfleiste, Eingabe, Seitenleiste mit Order-Liste und Pattern |
| `src/gfx.rs` | Framebuffer-Primitive: Rechtecke, Linien mit Glow, Text |
| `src/font.rs` | eigener 5×7-Bitmap-Font (erzeugt von `tools/make_font.py`) |
| `src/bin/fbshot.rs` | rendert Frames nativ als PPM-Bilder, zum Testen ohne Browser |

Tasten: Leertaste Play/Pause, S Stop, O Datei, V Ansicht, 3 Spektrum 2D/3D, P Farben, L Loop, F Vollbild.

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

```sh
RUSTC_BOOTSTRAP=1 rustc --edition 2021 --target wasm32-unknown-unknown --crate-type cdylib \
  -C opt-level=3 -C target-feature=+simd128 -C panic=abort -C strip=symbols \
  -o www-fb/mandel.wasm kernel/mandel_wasm.rs
```
