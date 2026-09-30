//! `audio_dump --out DIR`: writes every Deadfall sound as a .wav plus `table.txt` (peak, RMS, centroid,
//! duration, decay per cue) so the sound set can be inspected without the game.
use deadfall::client::audio::{self, Sfx};
use std::fmt::Write as _;
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out = args
        .iter()
        .position(|a| a == "--out")
        .and_then(|i| args.get(i + 1))
        .map_or_else(|| PathBuf::from("audio_dump"), PathBuf::from);
    std::fs::create_dir_all(&out).expect("create output directory");
    let started = std::time::Instant::now();
    let bank = audio::render();
    let secs = started.elapsed().as_secs_f32();
    let mut table = String::new();
    let _ = writeln!(table, "render time {secs:.2} s, rate {} Hz, {} pan steps", audio::RATE, audio::PAN_STEPS);
    let _ = writeln!(
        table,
        "{:>3} {:<18} {:>4} {:>5} {:>6} {:>6} {:>8} {:>6} {:>6}",
        "idx", "cue", "vars", "dir", "peak", "rms", "centroid", "secs", "decay"
    );
    let mut total = 0usize;
    for (i, &sfx) in Sfx::ALL.iter().enumerate() {
        let samples = audio::cue_samples(sfx);
        let s = audio::analyse(&samples);
        let _ = writeln!(
            table,
            "{:>3} {:<18} {:>4} {:>5} {:>6.3} {:>6.3} {:>8.0} {:>6.2} {:>6.2}",
            i,
            sfx.name(),
            bank.sfx[i].len(),
            if audio::is_directional(sfx) { "pan" } else { "-" },
            s.peak,
            s.rms,
            s.centroid,
            s.seconds,
            s.decay
        );
        for (v, wav) in bank.sfx[i].iter().enumerate() {
            total += wav.len();
            let name = out.join(format!("{:03}_{}_v{}.wav", i, sfx.name(), v));
            std::fs::write(name, wav).expect("write wav");
        }
    }
    let stems = audio::stem_samples();
    for (i, wav) in bank.stems.iter().enumerate() {
        total += wav.len();
        std::fs::write(out.join(format!("stem{i}.wav")), wav).expect("write stem");
        let s = audio::analyse(&stems[i]);
        let _ = writeln!(
            table,
            "stem{i} peak {:.3} rms {:.3} centroid {:.0} secs {:.1}",
            s.peak, s.rms, s.centroid, s.seconds
        );
    }
    let _ = writeln!(table, "total WAV bytes {total}");
    std::fs::write(out.join("table.txt"), &table).expect("write table");
    print!("{table}");
}
