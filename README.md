# Lightable

Open-source 2D drafting app in Rust ([lightable.dev](https://lightable.dev)), built on libraries you can use on their own:

| Crate | What it is |
|---|---|
| [`lightable-dxf`](crates/lightable-dxf) | Reads and writes DXF without losing data (for crates.io) |
| [`lightable-geom`](crates/lightable-geom) | Pure 2D geometry: bulges, curves, transforms, fills (for crates.io) |
| [`lightable-render`](crates/lightable-render) | Drawing → styled 2D primitives, SVG and GPU geometry |
| [`lightable-diff`](crates/lightable-diff) | Compares two drawings on their data |
| [`lightable`](crates/lightable) | The app (desktop and browser) |

 See [docs/SPEC.md](docs/SPEC.md) for the plan and [CHANGELOG.md](CHANGELOG.md) for progress.

```sh
cargo test --workspace
cargo run --release -p lightable -- tests/corpus/floor_plan__ezdxf__R2018.dxf   # viewer
cargo run --release -p lightable -- info tests/corpus/ac1015_floor_plan.dxf    # summary
```

Viewer: drag (middle or left button) to pan, scroll or pinch to zoom around the cursor, double-click the middle button or press "Zoom extents" to see everything, and tick layers on and off. Drop a DXF file on the window to open it.

Compare two revisions: `cargo run --release -p lightable -- --diff old.dxf new.dxf` (add `--side` for side by side), or drop two files on the window. The overlay shows added entities in green, deleted in red and changed in yellow; "Next change" zooms to each one, and "Export JSON" saves a change report. Without a window: `lightable diff old.dxf new.dxf [--json report.json | --svg overlay.svg]`.

Start it with no file (`cargo run --release -p lightable`) and it opens on an example: two revisions of a steel bracket drawing, with the changes listed in plain words. `--example plan` opens the floor-plan example and `--demo` plays a 30-second walkthrough (on the web: `?example=plan`, `?demo`). "Details" shows exact values, what is not compared yet, and the JSON and PNG exports.

Browser version: `tools/build_web.sh` (needs `cargo install wasm-bindgen-cli --version 0.2.129 --locked`, and wasm-opt from binaryen for the smallest build), then serve `target/web` (for example `python3 -m http.server -d target/web 8080`) and open it; drop one DXF to view it or two to compare them, or add `?example` to the address. Files are read in the page and never uploaded.

Rendering snapshots live in `tests/snapshots/`; after an intended rendering change, accept new ones with `UPDATE_SNAPSHOTS=1 cargo test -p lightable-render --test snapshots`.

The corpus in `tests/corpus/` is generated: hand-written files by `tools/gen_corpus.py`, the rest by `tools/gen_sources.py` (ezdxf) and re-saved by other CAD tools with `tools/convert_corpus.sh` — see [SOURCES.md](tests/corpus/SOURCES.md) and [COVERAGE.md](tests/corpus/COVERAGE.md). Put DXF files you can't redistribute in `tests/corpus-private/` (git-ignored); the tests load them too. Files re-saved by ODA File Converter (and LibreDWG's, made from an ODA DWG) live in `tests/corpus-private/converted/` and are not part of the public repository; `tools/convert_corpus.sh` writes them there.

## Licence

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
