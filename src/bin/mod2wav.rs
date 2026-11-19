//! Native test tool: render a MOD file to a 16-bit stereo WAV.
//! Usage: mod2wav <in.mod> <out.wav> [max_seconds]

use modplayer::replayer::Player;
use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: mod2wav <in.mod> <out.wav> [max_seconds]");
        std::process::exit(1);
    }
    let bytes = std::fs::read(&args[1]).expect("read mod");
    let max_secs: f32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(600.0);
    let sr = 44100u32;
    let mut player = Player::empty();
    let n = player.load(bytes.as_ptr(), bytes.len(), sr as f32);
    if n <= 0 {
        eprintln!("cannot parse MOD (error {n})");
        std::process::exit(2);
    }
    println!(
        "channels: {}, song length: {}",
        player.channels, player.song_len
    );

    let frames = 1024;
    let mut buf = vec![0f32; 2 * frames];
    let mut chans = vec![0f32; player.channels * frames];
    let mut pcm: Vec<i16> = Vec::new();
    while player.loops == 0 && (pcm.len() as f32) < max_secs * sr as f32 * 2.0 {
        player.render(buf.as_mut_ptr(), chans.as_mut_ptr(), frames);
        pcm.extend(buf.iter().map(|v| (v.clamp(-1.0, 1.0) * 32767.0) as i16));
    }
    let peak = pcm.iter().map(|v| v.unsigned_abs()).max().unwrap_or(0);
    println!(
        "rendered {:.1} s, peak {}",
        pcm.len() as f32 / 2.0 / sr as f32,
        peak
    );

    let mut f = std::fs::File::create(&args[2]).expect("create wav");
    let data_len = (pcm.len() * 2) as u32;
    let mut h = Vec::new();
    h.extend_from_slice(b"RIFF");
    h.extend_from_slice(&(36 + data_len).to_le_bytes());
    h.extend_from_slice(b"WAVEfmt ");
    h.extend_from_slice(&16u32.to_le_bytes());
    h.extend_from_slice(&1u16.to_le_bytes());
    h.extend_from_slice(&2u16.to_le_bytes());
    h.extend_from_slice(&sr.to_le_bytes());
    h.extend_from_slice(&(sr * 4).to_le_bytes());
    h.extend_from_slice(&4u16.to_le_bytes());
    h.extend_from_slice(&16u16.to_le_bytes());
    h.extend_from_slice(b"data");
    h.extend_from_slice(&data_len.to_le_bytes());
    f.write_all(&h).unwrap();
    let bytes: Vec<u8> = pcm.iter().flat_map(|v| v.to_le_bytes()).collect();
    f.write_all(&bytes).unwrap();
}
