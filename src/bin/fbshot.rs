//! Native test tool for the framebuffer renderer: plays a MOD (audio rendered natively,
//! not output), feeds the renderer exactly like the browser's worklet does and saves
//! frames as PPM images.
//! Usage: `fbshot <song.mod> <out-dir> [width height]`

use modplayer::api::*;
use modplayer::fft::Fft;
use modplayer::replayer::Player;
use std::io::Write;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 3 {
        eprintln!("usage: fbshot <song.mod> <out-dir> [width height]");
        std::process::exit(1);
    }
    let w: i32 = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(1280);
    let h: i32 = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(800);
    let song = std::fs::read(&a[1]).expect("read mod");
    std::fs::create_dir_all(&a[2]).unwrap();
    // the renderer's instance reads patterns from the api player
    unsafe { std::ptr::copy_nonoverlapping(song.as_ptr(), mp_mod_buffer(), song.len()) };
    mp_load(song.len(), 44100.0);
    vis_event(2);
    vis_set_playing(1);
    // our own player produces the audio, like the worklet
    let mut pl = Box::new(Player::empty());
    let nch = pl.load(song.as_ptr(), song.len(), 44100.0) as usize;
    let mut fft = Box::new(Fft::empty());
    fft.setup(4096);
    let mut ring = vec![0f32; 4096];
    let mut rpos = 0;
    let mut mix = vec![0f32; 2 * 1024];
    let mut ch = vec![0f32; nch * 1024];
    let shots: Vec<(f64, u32, bool, &str)> = vec![
        (6.0, 0, false, "osc"),
        (12.0, 1, false, "mandel"),
        (18.0, 2, false, "spec2"),
        (24.0, 2, true, "spec3"),
        (30.0, 3, false, "cube"),
        (34.0, 3, false, "cube2"),
    ];
    let mut view = 0u32;
    let mut spec3d = false;
    let mut t_ms = 0.0;
    let mut next_frame = 0.0;
    let mut si = 0;
    while si < shots.len() {
        pl.render(mix.as_mut_ptr(), ch.as_mut_ptr(), 1024);
        for i in 0..1024 {
            ring[rpos] = (mix[2 * i] + mix[2 * i + 1]) * 0.5;
            rpos = (rpos + 1) % 4096;
        }
        unsafe { fft.analyse_ring(ring.as_ptr(), 4096, rpos) };
        let slot = vis_slot_ptr();
        let n = nch.min(8);
        unsafe {
            for c in 0..n {
                std::ptr::copy_nonoverlapping(ch.as_ptr().add(c * 1024), slot.add(c * 1024), 1024);
            }
            for i in 0..1024 {
                *slot.add(n * 1024 + i) = mix[2 * i];
                *slot.add((n + 1) * 1024 + i) = mix[2 * i + 1];
            }
            std::ptr::copy_nonoverlapping(
                fft.power.as_ptr(),
                slot.add(vis_spec_offset() as usize),
                2049,
            );
        }
        t_ms += 1024.0 / 44.1;
        vis_commit(
            t_ms,
            pl.order_pos as u32,
            pl.row as u32,
            pl.pattern() as u32,
            pl.speed,
            pl.tempo,
            n as u32,
            44100.0,
        );
        let (st, sv, s3, name) = shots[si];
        if t_ms >= st * 1000.0 - 2500.0 {
            while view != sv {
                vis_key(118);
                view = (view + 1) % 4;
            }
            if spec3d != s3 {
                vis_key(51);
                spec3d = s3;
            }
            while next_frame <= t_ms {
                let p = vis_frame(next_frame, w, h, 1.0);
                next_frame += 1000.0 / 60.0;
                if next_frame >= st * 1000.0 {
                    let (fw, fh) = (vis_width(), vis_height());
                    let px = unsafe { std::slice::from_raw_parts(p, (fw * fh) as usize) };
                    let mut f = std::fs::File::create(format!("{}/{}.ppm", a[2], name)).unwrap();
                    write!(f, "P6\n{} {}\n255\n", fw, fh).unwrap();
                    let mut buf = Vec::with_capacity(px.len() * 3);
                    for &c in px {
                        buf.push((c & 255) as u8);
                        buf.push(((c >> 8) & 255) as u8);
                        buf.push(((c >> 16) & 255) as u8);
                    }
                    f.write_all(&buf).unwrap();
                    println!(
                        "{name}: {fw}x{fh} at {:.1} s (pos {}, row {})",
                        st, pl.order_pos, pl.row
                    );
                    si += 1;
                    break;
                }
            }
        } else {
            next_frame = t_ms;
        }
    }
}
