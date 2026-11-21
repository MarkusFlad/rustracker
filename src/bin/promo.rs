//! Renders the footage of the promo video natively: plays a MOD (audio rendered natively),
//! feeds the framebuffer renderer exactly like the browser's worklet does and pipes the frames
//! of each scene into its own ffmpeg process (caption overlay, upscale to 1080p, x264).
//! `tools/promo/make_video.sh` runs the whole pipeline (cards, footage, music, thumbnail).
//!
//! Usage: `promo <song.mod> <scenes.txt> <out-dir> [--stills t1,t2,...]`
//!
//! `scenes.txt`: one scene per line, `<start s> <end s> <view> <caption.png|-> [last]`, where
//! view is a number (0 oscilloscope … 5 lake), `3d` (3D spectrum) or `ui:N` (the whole
//! interface showing view N); `last` keeps the caption and fades the picture to black.
//! `--stills` additionally saves the frames at these times (seconds) as PPM images.

use modplayer::api::*;
use modplayer::fft::Fft;
use modplayer::replayer::Player;
use std::io::Write;
use std::process::{Command, Stdio};

const W: i32 = 1280;
const H: i32 = 720;
const FPS: f64 = 30.0;

struct Scene {
    start: f64,
    end: f64,
    view: i32,    // -1 = whole interface
    ui_view: u32, // the view shown inside the whole interface
    spec3d: bool,
    caption: String,
    last: bool, // the final scene: caption stays, picture fades to black
}

struct Feeder {
    pl: Box<Player>,
    fft: Box<Fft>,
    ring: Vec<f32>,
    rpos: usize,
    mix: Vec<f32>,
    ch: Vec<f32>,
    nch: usize,
    t_ms: f64,
}

impl Feeder {
    /// Renders audio blocks until `t_ms` passes `until`, committing each to the renderer.
    fn advance(&mut self, until: f64) {
        while self.t_ms < until {
            self.pl
                .render(self.mix.as_mut_ptr(), self.ch.as_mut_ptr(), 1024);
            for i in 0..1024 {
                self.ring[self.rpos] = (self.mix[2 * i] + self.mix[2 * i + 1]) * 0.5;
                self.rpos = (self.rpos + 1) % 4096;
            }
            unsafe { self.fft.analyse_ring(self.ring.as_ptr(), 4096, self.rpos) };
            let slot = vis_slot_ptr();
            let n = self.nch.min(8);
            unsafe {
                for c in 0..n {
                    std::ptr::copy_nonoverlapping(
                        self.ch.as_ptr().add(c * 1024),
                        slot.add(c * 1024),
                        1024,
                    );
                }
                for i in 0..1024 {
                    *slot.add(n * 1024 + i) = self.mix[2 * i];
                    *slot.add((n + 1) * 1024 + i) = self.mix[2 * i + 1];
                }
                std::ptr::copy_nonoverlapping(
                    self.fft.power.as_ptr(),
                    slot.add(vis_spec_offset() as usize),
                    2049,
                );
            }
            self.t_ms += 1024.0 / 44.1;
            let p = &self.pl;
            vis_commit(
                self.t_ms,
                p.order_pos as u32,
                p.row as u32,
                p.pattern() as u32,
                p.speed,
                p.tempo,
                n as u32,
                44100.0,
            );
        }
    }
}

/// Draws one frame: a single view, or the whole interface for view -1.
fn draw(view: i32, t: f64) -> *const u32 {
    if view < 0 {
        vis_frame(t, W, H, 1.0)
    } else {
        vis_view(view as u32, t, W, H, 1.4)
    }
}

/// The RGBA bytes of the last frame.
fn frame_bytes(p: *const u32) -> &'static [u8] {
    unsafe { std::slice::from_raw_parts(p as *const u8, (W * H * 4) as usize) }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let song = std::fs::read(&a[1]).expect("read mod");
    let scenes: Vec<Scene> = std::fs::read_to_string(&a[2])
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            let (view, ui_view) = match f[2] {
                "3d" => (2, 0),
                v if v.starts_with("ui:") => (-1, v[3..].parse().unwrap()),
                v => (v.parse().unwrap(), 0),
            };
            Scene {
                start: f[0].parse().unwrap(),
                end: f[1].parse().unwrap(),
                view,
                ui_view,
                spec3d: f[2] == "3d",
                caption: f[3].to_string(),
                last: f.get(4) == Some(&"last"),
            }
        })
        .collect();
    if a.len() < 4 {
        eprintln!("usage: promo <song.mod> <scenes.txt> <out-dir> [--stills t1,t2,...]");
        std::process::exit(1);
    }
    let out = &a[3];
    let stills: Vec<f64> = if a.len() > 5 && a[4] == "--stills" {
        a[5].split(',').map(|s| s.parse().unwrap()).collect()
    } else {
        vec![]
    };
    std::fs::create_dir_all(out).unwrap();

    unsafe { std::ptr::copy_nonoverlapping(song.as_ptr(), mp_mod_buffer(), song.len()) };
    mp_load(song.len(), 44100.0);
    vis_event(2);
    vis_set_playing(1);
    let mut pl = Box::new(Player::empty());
    let nch = pl.load(song.as_ptr(), song.len(), 44100.0) as usize;
    let mut fft = Box::new(Fft::empty());
    fft.setup(4096);
    let mut fd = Feeder {
        pl,
        fft,
        ring: vec![0.0; 4096],
        rpos: 0,
        mix: vec![0.0; 2048],
        ch: vec![0.0; nch * 1024],
        nch,
        t_ms: 0.0,
    };

    // the renderer runs at 60 fps like in the browser; every second frame goes to the video
    let step = 1000.0 / 60.0;
    let mut t = 0.0;
    let mut si = 0;
    let mut still_i = 0;
    let mut spec3d = false;
    while si < scenes.len() {
        let sc = &scenes[si];
        if sc.view == 2 && sc.spec3d != spec3d {
            vis_key(51);
            spec3d = sc.spec3d;
        }
        if sc.view < 0 {
            vis_view(sc.ui_view, t, W, H, 1.0); // selects the view the interface shows
        }
        let next_view = scenes.get(si + 1).map(|s| s.view).unwrap_or(-2);
        // frame numbers on the global 30 fps grid, so video time = song time in every scene
        let first = (sc.start * FPS).round() as usize;
        let frames = (sc.end * FPS).round() as usize - first;
        let tmp = format!("{}/scene{:02}.tmp.mp4", out, si);
        let mut ff = Some(spawn_ffmpeg(
            &tmp,
            &sc.caption,
            frames as f64 / FPS,
            sc.last,
        ));
        for f in 0..frames {
            let target = (first + f) as f64 / FPS * 1000.0;
            // draw the intermediate 60 fps frame too, so animations advance as in the browser
            while t + step < target {
                fd.advance(t);
                draw(sc.view, t);
                t += step;
            }
            t = target;
            fd.advance(t);
            // warm up the next view during the last two seconds (its frame is discarded)
            if sc.view >= 0
                && next_view >= 0
                && next_view != sc.view
                && target > sc.end * 1000.0 - 2000.0
            {
                draw(next_view, t);
            }
            let p = draw(sc.view, t);
            if let Some(ff) = ff.as_mut() {
                ff.stdin
                    .as_mut()
                    .unwrap()
                    .write_all(frame_bytes(p))
                    .unwrap();
            }
            while still_i < stills.len() && stills[still_i] * 1000.0 <= target {
                let name = format!("{}/still_{:05.1}.ppm", out, stills[still_i]);
                let mut file = std::fs::File::create(&name).unwrap();
                write!(file, "P6\n{} {}\n255\n", W, H).unwrap();
                let px = frame_bytes(p);
                let mut buf = Vec::with_capacity((W * H * 3) as usize);
                for c in px.chunks(4) {
                    buf.extend_from_slice(&c[..3]);
                }
                file.write_all(&buf).unwrap();
                eprintln!("still {} (scene {})", name, si);
                still_i += 1;
            }
        }
        if let Some(mut ff) = ff {
            drop(ff.stdin.take());
            let ok = ff.wait().unwrap().success();
            assert!(ok, "ffmpeg failed for scene {}", si);
            std::fs::rename(&tmp, format!("{}/scene{:02}.mp4", out, si)).unwrap();
            eprintln!("scene {} done ({} frames)", si, frames);
        }
        si += 1;
    }
}

/// ffmpeg reading raw RGBA frames on stdin; optional caption PNG faded in and out on top.
fn spawn_ffmpeg(path: &str, caption: &str, dur: f64, last: bool) -> std::process::Child {
    let mut args: Vec<String> = vec![
        "-y",
        "-loglevel",
        "error",
        "-f",
        "rawvideo",
        "-pix_fmt",
        "rgba",
        "-s",
        "1280x720",
        "-r",
        "30",
        "-i",
        "-",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    let filter = if caption != "-" {
        args.extend(["-framerate", "30", "-i", caption].map(String::from));
        let fout = (dur - 0.6).max(0.5);
        let (capfade, end) = if last {
            (
                String::new(),
                format!(",fade=out:st={:.3}:d=1.8", dur - 1.8),
            )
        } else {
            (
                format!(",fade=out:st={fout:.3}:d=0.45:alpha=1"),
                String::new(),
            )
        };
        let trim = dur + 1.0; // longer than the picture: the picture decides the length
        format!(
            "[0:v]scale=1920:1080:flags=lanczos[bg];[1:v]format=rgba,loop=-1:1,trim=duration={trim:.3},fade=in:st=0.15:d=0.6:alpha=1{capfade}[cap];[bg][cap]overlay=0:0:shortest=1{end},format=yuv420p"
        )
    } else {
        "[0:v]scale=1920:1080:flags=lanczos,format=yuv420p".to_string()
    };
    args.extend(
        [
            "-filter_complex",
            &filter,
            "-c:v",
            "libx264",
            "-crf",
            "18",
            "-preset",
            "veryfast",
            "-pix_fmt",
            "yuv420p",
            "-r",
            "30",
            path,
        ]
        .map(String::from),
    );
    Command::new("ffmpeg")
        .args(&args)
        .stdin(Stdio::piped())
        .spawn()
        .expect("ffmpeg")
}
