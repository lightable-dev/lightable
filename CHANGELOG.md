# Changelog

## Demo refinement (unreleased)

- The bracket example is now one engineering change (bolts M12 → M16) with its real consequences: clearance holes Ø13.5 → Ø17.5 (ISO 273) moved 5 mm inward for edge distance (EN 1993-1-8), the hole callout, the bolt note and four dimensions. The revision cloud surrounds only the bolt note. An expected-change inventory, written before comparing, is checked by a test both ways.
- Dimensions are compared by their measurement ("Dimension changed from 20 mm to 25 mm"), computed from the definition points when the file does not store it.
- Revision-markup layers: exactly REV, or starting with REV-, REV_, REVISION, TITLE or BORDER (REVIEW, REVOLVE and the like no longer count).
- Headlines contain no coordinates ("Corner chamfered 10 × 10 mm"); coordinates are in the details.
- Demo script rewritten around a pre-release review; every number and headline in it is computed from the comparison (`docs/demo/`).
- Web: while the app loads, the example's overlay and change list (made by the comparison at build time) and a download progress line; the app is 3.92 MB (1.59 MB compressed, was 6.62 / 2.41 MB) through a size profile, wasm-opt and dropping egui's emoji fonts; ready in about 12 s on "Fast 3G" (was 16 s), with the change list visible after 0.7 s.
- Fixed: the first view of a comparison was fitted before the change list settled its width, cutting off part of the drawing (most visibly the floor plan); the view now refits when the panes resize until it is moved.
- Fixed: demo zooms fitted to the previous pane size when a step switched side by side or Details; long headlines in the PNG report were cut off.
- `lightable diff A B --svg OUT` writes the overlay as SVG.
- Summary: "plus revision markup (M items, including K revision clouds)"; the bracket example adds "Example: the drafter clouded only the bolt note." JSON: `revision_cloud` per change, `summary.revision_clouds`.

## First-run experience (unreleased)

- The app opens on a comparison: a new default example, a steel bracket drawing Rev B → Rev C with exactly 4 design changes, a revision cloud and a title block; the floor plan stays as a second example in an example picker.
- Plain-words headlines for every change ("Circle Ø12 moved 5 mm right", "Circle diameter changed from 12 mm to 14 mm", "Text changed from M12 to M16"), with units from `$INSUNITS`; revision clouds and title-block changes counted as revision markup.
- Layered UI: summary and legend with no clicks; change list; side by side; everything else behind "Details". One hint at a time.
- Demo mode (`?demo`, `--demo`): a captioned 30-second walkthrough ending on "Free. Open source. Your files never leave your computer. lightable.dev".
- Fixed: text on screen was drawn about one text height too low.

## Public repository (unreleased)

- `lightable-dxf`: every public item documented; `missing_docs` enforced.
- ODA- and LibreDWG-converted corpus files moved out of the repository (to the git-ignored `tests/corpus-private/converted/`); tests and CI pass with the 63 public files.
- GitHub Pages workflow for the browser version at lightable.dev.

## Rename to Lightable (unreleased)

- Crates renamed to `lightable-dxf`, `lightable-geom`, `lightable-diff`, `lightable-render`, `lightable-core` and the app `lightable`; the app is called Lightable.
- `lightable-geom` and `lightable-dxf` 0.1.0 prepared for crates.io: metadata, licence files, READMEs (run as doctests), full docs for `lightable-geom`; `cargo publish --dry-run` passes. Not published.
- Minimum Rust 1.88 (was wrongly 1.85), checked in CI.
- JSON diff report format is now `lightable-diff/1`.

## Preview preparation (unreleased)

- Visual diff: a change inside a block definition lists and highlights every reference that shows it (also through nested blocks), and "Next change" zooms to them.
- "Not compared yet" note in the app, the JSON report (`not_compared`), the CLI and the PNG report: text styles, dimension styles and linetype definitions (with whether they differ), other tables, other header settings, objects, anonymous blocks.
- Hatches are drawn: boundary lines plus fill (solid in the entity colour, pattern hatches as a dim tint until patterns are drawn). New `lightable_dxf::hatch` boundary reader and `lightable_geom::fill::even_odd_triangles`; checked against ezdxf's own flattening (`tests/hatch/`).
- Built-in example: two revisions of a floor plan made with ezdxf ("Try the example", `lightable --example`, `?example` on the web).
- "Export PNG report": the overlay, change list and "not compared" note as one image with a footer naming the app; `lightable --diff A B --report`.
- "Feedback" button: opens a pre-filled GitHub issue in the browser (version and platform only; nothing is sent by the app). No tracking.
- Web build: `tools/build_web.sh` (wasm-bindgen 0.2.129) and `web/index.html` with a strict Content Security Policy. Tested in Chrome 154, Firefox 156 and Safari 18.6 with corpus files; no request leaves the page after a file is opened (see DECISIONS.md).
- Exports download in the browser; on the desktop they are saved next to drawing B (or in Downloads).

## M4b — Visual diff (unreleased)

- New UI-free crate `lightable-diff`: compares two drawings on their data (entities in model and paper space, named blocks, layers, units and linetype scale), matching by handle when handles are trustworthy and otherwise by type, layer and geometry within a tolerance.
- Changes are classified as added, deleted or changed, with details in plain words ("moved by (10, 5)", "layer changed from WALLS to DOORS", `text changed from "LIVING" to "LOUNGE"`, "radius changed from 4 to 6"). Types the viewer cannot draw are marked "not shown"; anything not interpreted is marked "uncertain". Handles, `$HANDSEED`, timestamps and entity order are ignored; entities drawn identically by other entities (a polyline split into lines) are listed as "redrawn", not as changes.
- Machine-readable JSON report (`lightable-diff/1`).
- App compare mode: overlay (green added, red deleted, yellow changed), side by side with shared pan/zoom, change list, "Next change" zooming to each change, JSON export. `lightable --diff A B [--side]`, drop two files, or "Compare with…"; `lightable diff A B [--json OUT]` on the command line.
- Test pairs made with ezdxf and FreeCAD (`tests/diff/`, `tools/gen_diff_pairs.py`): 28/28 expected changes found, 0 extra (precision and recall 1.000); renumbered, other-version and FreeCAD re-saves report no geometry changes.
- `lightable-render`: `Item::entity`, `build_with`, `build_block`.

## M4 — Viewer (unreleased)

- `lightable-render` (no UI dependencies): drawing → scene of styled primitives (layers, colours, linetypes, nested blocks and arrays, OCS), SVG output, chunked GPU geometry with a coarse level of detail, and pan/zoom maths.
- `lightable-geom`: OCS (arbitrary axis algorithm) and 3D affine transforms.
- `lightable` viewer (eframe + glow): pan, zoom around the cursor, zoom extents, layers on/off, colours, linetypes and text; drag-and-drop to open; `--bench` frame timing and `--shot` screenshots.
- 100k-entity file: 7.5 ms average, 15.2 ms 95th-percentile frame while panning and zooming (60 fps).
- Golden SVG snapshots for all 198 corpus files (`tests/snapshots/`).
- WebAssembly build of `lightable` (CI job), with `index.html` for trunk.
- `lightable info` replaces the bare-file summary; `MText::plain_lines` in `lightable-dxf`.
- DECISIONS.md: M4 decisions and rules for M5 saving.

## M3 — Writer and round-trip (unreleased)

- DXF writer for R12 and R2018 (`Drawing::to_bytes`, `write_file`, `generate_bytes`), with handles on every object and, in R2018, owner links, block records and layouts computed from the structure.
- Save-back (`save_bytes`, `save_file`): an unchanged drawing is written back byte-for-byte, including stray records, the unparsed tail and original text; all 198 corpus files do.
- R12 downgrades: LWPOLYLINE → POLYLINE, ELLIPSE/SPLINE → polylines (`lightable_geom::curve`), MTEXT → TEXT, true colour → nearest ACI, R12-valid names; everything lost is reported.
- Bytes invalid in the code page are kept (as U+F7xx) and written back unchanged.
- Round-trip tests over the corpus (byte-identical save-back; generated files load cleanly and are a fixpoint; R2018 keeps entity content).
- `tools/validate_written.py`: all 396 generated files (198 × R12/R2018) open cleanly in ODA File Converter and FreeCAD and in our reader; report in `tests/corpus/WRITE_VALIDATION.md`.
- `lightable convert`, `generate` and `save` commands.

## M2 part B — nothing dropped, encodings, fuzzing (unreleased)

- Stray and duplicate records (e.g. an extra `ENDBLK`) are kept in `Drawing::stray` with their position; an unreadable tail is kept in `Drawing::unparsed_tail`. Warnings unchanged.
- `ATTDEF` parsed.
- `$DWGCODEPAGE` decoding for pre-2007 files (ANSI_874, 932, 936, 949, 950, 1250–1258) via `encoding_rs`; `Drawing::encoding` records the result.
- `\U+XXXX`, `\M+nXXXX` and `%%` codes decoded in TEXT, ATTRIB, ATTDEF, MTEXT and dimension text; the original string is kept when it differs. Public `decode_text` / `decode_mtext`.
- cargo-fuzz targets (`tokenizer`, `reader`, `text`) seeded from the corpus, `tools/fuzz.sh`, and replayed regression inputs. A 1-hour run of all three found no crashes (223 M executions in total).
- All corpus files load with zero warnings, except the 18 known LibreDWG files (duplicate records) and the two deliberately damaged fixtures.

## M2 part A — performance, entities, blocks, tables (unreleased)

- Streaming reader with borrowed tokens: the 50 MB benchmark file loads in about 0.8–1.05 s (was 3.1 s). New benchmark: `cargo bench -p lightable-dxf --bench load`.
- R13 and R14 added to `Version`.
- New typed entities: `LWPOLYLINE` (bulges, widths, vertex ids), `ELLIPSE`, `POINT`, `TEXT`, `MTEXT`, `INSERT` (arrays, attributes), `ATTRIB`, `DIMENSION` (linear, aligned, angular, diameter, radius, 3-point angular, ordinate), `SPLINE`, and `POLYLINE`/`VERTEX`/`SEQEND` sequences.
- `BLOCKS` section parsed into `Drawing::blocks` (`Table<Block>`), including nested references and child sequences inside blocks.
- New typed tables: linetypes (with complex elements), text styles, dim styles, block records.
- `lightable-geom`: `bulge::arc_from_bulge` and `bulge_from_sweep`.
- Corpus acceptance test: all real-writer files load with no warnings except encoding, apart from 18 LibreDWG files whose duplicate records are reported.

## Unreleased (pre-M2)

- Recorded M1 design decisions in `docs/DECISIONS.md`; dropped the `dxf` crate evaluation from the spec.
- Added `LICENSE-MIT` and `LICENSE-APACHE`.
- Committed the corpus generator as `tools/gen_corpus.py`.
- Corpus tests also load files from the git-ignored `tests/corpus-private/` when present.
- Real-writer corpus: 13 drawings generated with ezdxf (`tools/gen_sources.py`) and re-saved by ODA File Converter, FreeCAD and LibreDWG (`tools/convert_corpus.sh`) — 198 files across R12, R14 and R2000–R2018. Provenance in `tests/corpus/SOURCES.md`, coverage in `tests/corpus/COVERAGE.md` (`tools/corpus_coverage.py`).
- A ~50 MB mixed-entity speed-test file can be generated into `tests/corpus-private/`.
- The exhaustive truncation/corruption sweep now runs only on the small hand-written files.

## M1 — Tokenizer and model (unreleased)

- Cargo workspace with `lightable-dxf`, `lightable-geom`, `lightable-core`, `lightable-render` and `lightable` (the last four are stubs).
- `lightable-dxf` tokenizer: group-code/value pairs with LF or CRLF line endings, padded codes, empty values, a UTF-8 BOM and line numbers; binary DXF is rejected with a clear error.
- Reader: `HEADER` (all variables kept, typed accessors for version, code page, units, extents, current layer, text style and dim style), `TABLES` (LAYER parsed; other tables kept verbatim), `ENTITIES` (`LINE`, `CIRCLE`, `ARC` parsed; everything else kept verbatim as `Unknown`); `CLASSES`, `BLOCKS`, `OBJECTS` and unknown sections kept verbatim.
- Recoverable errors: a malformed entity becomes `Unknown` plus a warning; truncated files keep what was read; missing `ENDSEC`/`ENDTAB`/`EOF` are warnings.
- Corpus of 13 hand-written DXF files in `tests/corpus/`, corpus tests, and a truncation/corruption sweep that checks the reader never panics.
- GitHub Actions CI: `fmt --check`, `clippy -D warnings`, `test` on Linux, Windows and macOS.
