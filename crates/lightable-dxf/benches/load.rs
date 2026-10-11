//! Load-time benchmark: `cargo bench -p lightable-dxf --bench load`.
//!
//! Times `read_bytes` on `tests/corpus-private/big_mixed_50mb__ezdxf__R2018.dxf`
//! when present (see tests/corpus/SOURCES.md to regenerate it), otherwise on
//! a synthetic ~50 MB drawing built in memory. M2 target: under 2 s.
//! Set `CAD_BENCH_FILE` to time another file.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const RUNS: usize = 5;
const TARGET: Duration = Duration::from_secs(2);

fn main() {
    let (label, bytes) = input();
    let mb = bytes.len() as f64 / 1e6;
    let mut times = Vec::with_capacity(RUNS);
    let mut entities = 0;
    for _ in 0..RUNS {
        let start = Instant::now();
        let (drawing, _warnings) =
            lightable_dxf::read_bytes(&bytes).expect("benchmark input loads");
        times.push(start.elapsed());
        entities = drawing.entities.len();
        // Dropping the drawing is not part of the load.
        std::mem::drop(std::hint::black_box(drawing));
    }
    times.sort();
    let median = times[RUNS / 2];
    println!(
        "{label}: {mb:.1} MB, {entities} entities: min {:.3} s, median {:.3} s ({:.0} MB/s), target < {} s",
        times[0].as_secs_f64(),
        median.as_secs_f64(),
        mb / median.as_secs_f64(),
        TARGET.as_secs()
    );
    if median > TARGET {
        println!("warning: median load time is over the target");
    }
}

fn input() -> (String, Vec<u8>) {
    let path = std::env::var_os("CAD_BENCH_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/corpus-private/big_mixed_50mb__ezdxf__R2018.dxf")
        });
    match std::fs::read(&path) {
        Ok(bytes) => (path.display().to_string(), bytes),
        Err(_) => ("synthetic".into(), synthetic(50_000_000).into_bytes()),
    }
}

/// A mixed-entity drawing of about `target` bytes, shaped like an R2018 file.
fn synthetic(target: usize) -> String {
    let mut s = String::with_capacity(target + 4096);
    s.push_str(
        "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n",
    );
    let mut i: u64 = 0;
    let mut seed: u64 = 42;
    let mut rnd = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % 100_000_000) as f64 / 1000.0
    };
    while s.len() < target {
        let common = format!(
            "5\n{:X}\n330\n1F\n100\nAcDbEntity\n8\nBIG{:02}\n",
            i + 0x100,
            i % 20
        );
        let (x, y) = (rnd(), rnd());
        let _ = match i % 8 {
            0..=2 => write!(
                s,
                "0\nLINE\n{common}100\nAcDbLine\n10\n{x}\n20\n{y}\n30\n0.0\n11\n{}\n21\n{}\n31\n0.0\n",
                x + 5.0,
                y + 5.0
            ),
            3 => write!(
                s,
                "0\nCIRCLE\n{common}100\nAcDbCircle\n10\n{x}\n20\n{y}\n30\n0.0\n40\n{}\n",
                rnd() / 1000.0 + 0.5
            ),
            4 => write!(
                s,
                "0\nARC\n{common}100\nAcDbCircle\n10\n{x}\n20\n{y}\n30\n0.0\n40\n3.5\n100\nAcDbArc\n50\n{}\n51\n{}\n",
                rnd() % 360.0,
                rnd() % 360.0
            ),
            5 => write!(
                s,
                "0\nLWPOLYLINE\n{common}100\nAcDbPolyline\n90\n4\n70\n1\n43\n0.0\n10\n{x}\n20\n{y}\n10\n{}\n20\n{y}\n42\n0.5\n10\n{}\n20\n{}\n10\n{x}\n20\n{}\n",
                x + 5.0,
                x + 5.0,
                y + 5.0,
                y + 5.0
            ),
            6 => write!(
                s,
                "0\nTEXT\n{common}100\nAcDbText\n10\n{x}\n20\n{y}\n30\n0.0\n40\n1.0\n1\nT{i}\n100\nAcDbText\n"
            ),
            _ => write!(
                s,
                "0\nINSERT\n{common}100\nAcDbBlockReference\n2\nMARK\n10\n{x}\n20\n{y}\n30\n0.0\n"
            ),
        };
        i += 1;
    }
    s.push_str("0\nENDSEC\n0\nEOF\n");
    s
}
