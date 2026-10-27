//! The demo's large layout: a distribution centre of 52 rack rows of 110
//! bays (each bay a frame, two beam lines and a location label) and columns
//! on a 12 m grid, 23,181 objects in Rev A. Rev B relabels one bay
//! (R1-065 -> QC-01), moves one bay frame 300 mm right and adds a column.
//!
//! Written here, deterministically (no randomness), as plain ASCII DXF, so
//! the test that checks it against `assets/depot-inventory.json` always
//! runs, and `lightable demo-cues` writes the same files for the recorder.
//! No dependencies: crates/lightable-diff/tests/examples.rs includes this
//! file too.

use std::fmt::Write;

/// Rows of racks, bays per row, and the bay frame in millimetres.
const ROWS: usize = 52;
const BAYS: usize = 110;
const BAY_W: f64 = 2700.0;
const BAY_D: f64 = 1100.0;
const AISLE: f64 = 3200.0;

/// The layout as DXF text: Rev A, or Rev B when `rev_b`.
pub fn depot(rev_b: bool) -> String {
    let mut out = String::new();
    let mut handle = 0x100_u32;
    let mut next = || {
        handle += 1;
        format!("{handle:X}")
    };
    out.push_str("0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1015\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n");
    out.push_str("0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nLAYER\n70\n5\n");
    for (name, color) in [
        ("RACKS", 7),
        ("BEAMS", 8),
        ("LABELS", 2),
        ("COLUMNS", 4),
        ("BUILDING", 7),
    ] {
        let _ = write!(
            out,
            "0\nLAYER\n2\n{name}\n70\n0\n62\n{color}\n6\nCONTINUOUS\n"
        );
    }
    out.push_str("0\nENDTAB\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n");
    let frame = |out: &mut String, h: String, x: f64, y: f64| {
        let _ = write!(out, "0\nLWPOLYLINE\n5\n{h}\n8\nRACKS\n90\n4\n70\n1\n");
        for (px, py) in [
            (x, y),
            (x + BAY_W, y),
            (x + BAY_W, y + BAY_D),
            (x, y + BAY_D),
        ] {
            let _ = write!(out, "10\n{px}\n20\n{py}\n");
        }
    };
    let line = |out: &mut String, h: String, layer: &str, a: (f64, f64), b: (f64, f64)| {
        let _ = write!(
            out,
            "0\nLINE\n5\n{h}\n8\n{layer}\n10\n{}\n20\n{}\n30\n0\n11\n{}\n21\n{}\n31\n0\n",
            a.0, a.1, b.0, b.1
        );
    };
    let circle = |out: &mut String, h: String, x: f64, y: f64| {
        let _ = write!(
            out,
            "0\nCIRCLE\n5\n{h}\n8\nCOLUMNS\n10\n{x}\n20\n{y}\n30\n0\n40\n250\n"
        );
    };
    for r in 0..ROWS {
        // Back-to-back pairs of rows with an aisle between pairs.
        let y = (r / 2) as f64 * (2.0 * BAY_D + AISLE) + (r % 2) as f64 * BAY_D;
        for b in 0..BAYS {
            let x = b as f64 * BAY_W;
            // Rev B: a fire-escape gap, one frame moved 300 mm right.
            let shift = if rev_b && (r, b) == (30, 12) {
                300.0
            } else {
                0.0
            };
            frame(&mut out, next(), x + shift, y);
            for beam in [y + 100.0, y + BAY_D - 100.0] {
                line(
                    &mut out,
                    next(),
                    "BEAMS",
                    (x + 50.0, beam),
                    (x + BAY_W - 50.0, beam),
                );
            }
            let label = if rev_b && (r, b) == (17, 64) {
                "QC-01".to_owned()
            } else {
                format!(
                    "{}{}-{:03}",
                    (b'A' + (r % 26) as u8) as char,
                    r / 26 + 1,
                    b + 1
                )
            };
            let (cx, cy) = (x + BAY_W / 2.0, y + BAY_D / 2.0);
            let _ = write!(
                out,
                "0\nTEXT\n5\n{}\n8\nLABELS\n10\n{cx}\n20\n{cy}\n30\n0\n40\n180\n1\n{label}\n72\n1\n11\n{cx}\n21\n{cy}\n31\n0\n73\n2\n",
                next()
            );
        }
    }
    let width = BAYS as f64 * BAY_W;
    let depth = (ROWS / 2) as f64 * (2.0 * BAY_D + AISLE);
    let _ = write!(
        out,
        "0\nLWPOLYLINE\n5\n{}\n8\nBUILDING\n90\n4\n70\n1\n",
        next()
    );
    for (px, py) in [
        (-6000.0, -6000.0),
        (width + 6000.0, -6000.0),
        (width + 6000.0, depth + 6000.0),
        (-6000.0, depth + 6000.0),
    ] {
        let _ = write!(out, "10\n{px}\n20\n{py}\n");
    }
    let mut gx = 0.0;
    while gx <= width {
        let mut gy = 0.0;
        while gy <= depth {
            circle(&mut out, next(), gx, gy - 400.0);
            gy += 12000.0;
        }
        gx += 12000.0;
    }
    if rev_b {
        circle(&mut out, next(), width + 3000.0, depth / 2.0);
    }
    out.push_str("0\nENDSEC\n0\nEOF\n");
    out
}
