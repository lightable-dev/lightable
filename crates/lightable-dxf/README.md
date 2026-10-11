# lightable-dxf

Reads and writes ASCII DXF drawings in Rust, without losing what it does not understand.

- **Reads** R12 to 2018 files: header, tables (layers, linetypes, text styles, dimension styles, block records), blocks and entities, into a plain Rust model.
- **Typed entities:** LINE, CIRCLE, ARC, LWPOLYLINE, POLYLINE/VERTEX, ELLIPSE, SPLINE, POINT, TEXT, MTEXT, INSERT (with attributes and arrays), ATTRIB, ATTDEF and DIMENSION. Anything else is kept verbatim as `Unknown`; a typed view of HATCH boundaries is in `hatch`.
- **Nothing is dropped:** unknown entities, tables and sections, stray records and even an unreadable tail are kept, so a drawing you only read is written back byte for byte (`save_bytes`).
- **Writes** R12 or R2018 (`to_bytes`, `write_file`), with handles and owner links computed from the structure, and reports anything an R12 file cannot hold.
- **Text:** pre-2007 code pages (`$DWGCODEPAGE`) and `\U+`, `\M+` and `%%` escapes are decoded; the original bytes are kept.
- **Never panics on bad input:** problems that affect only part of a file come back as warnings. Fuzzed for an hour per target.
- **Fast:** a 50 MB drawing loads in about a second.

```rust
use lightable_dxf::{EntityKind, Version};

let text = "0\nSECTION\n2\nENTITIES\n0\nLINE\n8\nWALLS\n10\n0\n20\n0\n11\n5\n21\n0\n0\nENDSEC\n0\nEOF\n";
let (drawing, warnings) = lightable_dxf::read_str(text)?;
assert!(warnings.is_empty());

for e in &drawing.entities {
    if let EntityKind::Line(line) = &e.kind {
        println!("{} line from {:?} to {:?}", e.layer, line.start, line.end);
    }
}

// Unchanged: written back exactly as read.
let (same, _) = drawing.save_bytes();
assert_eq!(same, text.as_bytes());

// Or as a new R2018 file.
let (bytes, _warnings) = drawing.to_bytes(Version::R2018)?;
assert!(lightable_dxf::read_bytes(&bytes).is_ok());
# Ok::<(), lightable_dxf::DxfError>(())
```

Files: `lightable_dxf::read_file(path)` and `drawing.save_file(path)` / `write_file(path, version)`.

This crate is part of [Lightable](https://lightable.dev), an open-source 2D drafting app; it has no UI dependencies and can be used on its own. Clean-room: built from public DXF documentation and test files.

## Licence

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
