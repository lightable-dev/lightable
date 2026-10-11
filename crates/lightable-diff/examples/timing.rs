//! Times diffing two drawings: `timing A.dxf B.dxf [--renumber]`. With
//! `--renumber`, B's entities get new handles and reversed order first, so
//! nothing can be matched by handle (the slowest case).
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let t = std::time::Instant::now();
    let (a, _) = lightable_dxf::read_file(&args[0]).unwrap();
    let (mut b, _) = lightable_dxf::read_file(&args[1]).unwrap();
    eprintln!("read both: {:?}", t.elapsed());
    if args.get(2).is_some_and(|f| f == "--renumber") {
        b.entities.reverse();
        for (k, e) in b.entities.iter_mut().enumerate() {
            e.handle = Some(lightable_dxf::Handle(0x100000 + k as u64));
        }
    }
    let t = std::time::Instant::now();
    let d = lightable_diff::diff(&a, &b, &lightable_diff::DiffOptions::default());
    eprintln!(
        "diff: {:?}; {} changes, {} unchanged, handles trusted: {}",
        t.elapsed(),
        d.changes.len(),
        d.unchanged,
        d.handles_trusted
    );
}
