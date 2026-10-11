# 2D CAD Build Spec: DXF Core + Drafting App

> **Core principle:** take a professional task that currently needs expertise, multiple operations and manual checking, and make the outcome achievable through one clear request, while keeping transparency, control and independent validation.

## Overview

Build Lightable, a free, open-source 2D CAD app in Rust that works as a companion alongside existing CAD applications: it reads and writes DXF, so drawings move freely between Lightable and the tools a team already uses, and its command-line workflow is familiar to anyone who has drafted with existing CAD applications. Step 1 is a standalone DXF library; Step 2 is the drafting app built on top of it.

**In scope for v0.1**

- DXF read/write (R12 and AC1015/2000 through AC1032/2018 ASCII)
- 2D entities: lines, polylines, circles, arcs, text, dimensions, blocks
- Layers, colors, linetypes, units
- 20 core drafting commands with a command line, snapping and undo
- Export to PDF and SVG
- Visual diff between two drawings (M4b)
- A WebAssembly build of the viewer and visual diff

**Out of scope for v0.1**

- DWG (planned for v0.2 via LibreDWG bindings)
- 3D, solids, paper-space layouts beyond one default, xrefs, hatches with complex patterns, plugins/scripting

**Rules that apply everywhere**

- Clean-room: build from public DXF documentation and your own test files. Never use another CAD vendor's code, icons or branding, and never use the names of existing CAD applications in the product.
- Pick your own product name before the first public release. "Working name" in this spec is a placeholder. (Chosen: **Lightable**; see DECISIONS.md.)
- Hand each milestone to Claude Code as its own task, with the acceptance criteria from the Milestones section pasted in.

## Why Lightable

Lightable is a companion that works alongside existing CAD applications, not a replacement for them: it reads and writes the same DXF files, so it can sit next to whatever a team already uses. What it focuses on:

- **Small and fast.** A single small download that starts instantly and stays responsive on large drawings (the DXF reader already loads 50 MB in under a second).
- **Free and fully offline, with no account.** Every feature works without signing in or connecting to the internet.
- **Visual diff between drawing revisions.** Open two versions of a drawing and see what was added, deleted and changed (milestone M4b).
- **Modern scripting (later).** A scripting interface built on the same command engine, planned after v0.1.
- **AI commands (later).** Natural-language requests turned into real, previewable commands, planned after v0.1.
- **No telemetry without consent.** Nothing leaves the user's machine unless they explicitly choose to send it (for example, via "Send feedback").

## Architecture

One Cargo workspace with five crates. `lightable-dxf` and `lightable-geom` have no UI dependencies, so other projects can use them on their own.

`lightable-render` draws from the document held in `lightable-core`; geometry maths lives only in `lightable-geom`, so commands and rendering share one tested implementation.

```text
cad/
├── Cargo.toml          # workspace
├── crates/
│   ├── lightable-dxf/        # Step 1
│   ├── lightable-geom/
│   ├── lightable-core/
│   ├── lightable-render/
│   └── lightable/        # binary
├── tests/corpus/       # DXF test files
├── docs/SPEC.md        # this spec
└── CLAUDE.md
```

## Step 1: DXF core library

The library (`lightable-dxf`) turns a DXF file into a clean Rust data model and back, with no data loss for anything it doesn't understand.

### Data model

- `Drawing` holds `Header`, `Tables`, `Blocks`, `Entities`, `Objects`.
- `Header`: units (`$INSUNITS`), extents, current layer, text style, dim style, codepage, version.
- `Tables`: layers, linetypes, text styles, dim styles, block records. Each keyed by name, case-insensitive.
- `Entity` = common fields (`handle`, `layer`, `color`, `linetype`, `lineweight`, `visible`) + `kind: EntityKind`.
- `EntityKind` enum for v0.1: `Line`, `LwPolyline` (with bulges), `Circle`, `Arc`, `Ellipse`, `Point`, `Text`, `MText`, `Insert` (block reference), `Dimension` (linear, aligned, radial, angular), `Spline` (read and display only).
- `Color`: ACI index (1–255), ByLayer, ByBlock, or 24-bit true colour.
- Anything unrecognised is kept as `Unknown { type_name, group_codes: Vec<(i16, Value)> }` and written back unchanged. This is the most important rule in the library.
- Use `f64` everywhere for coordinates; never `f32`.

### Reader

1. Tokenizer: read pairs of lines (group code, value). Handle CRLF and LF, leading spaces, and empty values.
2. Section parser for `HEADER`, `CLASSES`, `TABLES`, `BLOCKS`, `ENTITIES`, `OBJECTS`. Skip unknown sections without failing.
3. Text encoding: versions before 2007 use `$DWGCODEPAGE` (e.g. ANSI_1252); 2007 and later are UTF-8. Decode `\U+XXXX` escapes in strings.
4. Errors are recoverable where possible: one bad entity becomes `Unknown` plus a warning, not a failed load. Return `(Drawing, Vec<Warning>)`.
5. Binary DXF: detect its header and return a clear "not supported yet" error.

### Writer

- Write two targets: R12 (widest compatibility) and AC1032 (2018).
- Assign and preserve handles; keep owner/handle references consistent.
- Write `Unknown` items back byte-for-byte in their original position.
- Output must open without warnings in at least two other DXF readers (existing CAD applications or a free DWG/DXF viewer).

### Public API (target shape)

```rust
let (drawing, warnings) = lightable_dxf::read_file("plan.dxf")?;
for e in drawing.entities.iter().filter(|e| e.layer == "WALLS") { /* ... */ }
drawing.write_file("out.dxf", lightable_dxf::Version::R2018)?;
```

### Testing

- Corpus: 50+ DXF files from different sources and versions, saved in `tests/corpus/` (use only files you are allowed to redistribute; otherwise keep a private corpus).
- Round-trip test: read → write → read gives an equal `Drawing` for every corpus file.
- Fuzzing with `cargo-fuzz` on the tokenizer and reader: no panics, ever.
- Golden renders: each corpus file rendered to SVG and compared against a checked-in snapshot, so regressions show up as image diffs.
- Performance: a 50 MB DXF loads in under 2 seconds on a mid-range laptop.

## Step 2: Drafting app

The app should feel familiar within five minutes to anyone who has used existing CAD applications: type a command, answer the prompts, see the result. Everything is driven through the command line; toolbars are shortcuts to commands, never separate code paths.

### Canvas

- Infinite 2D canvas, dark background by default, world coordinates in `f64`, rendering in `f32` relative to the view centre to avoid precision jitter far from origin.
- Pan with middle mouse drag; zoom with wheel around the cursor; double-click middle = zoom extents.
- Crosshair cursor with a small pick box; coordinates shown live in the status bar.
- Spatial index (R-tree) over entity bounding boxes for picking, snapping and culling.
- Target: 100,000 entities at 60 fps pan/zoom.

### Command line

- Docked at the bottom. Type a command or alias, press Enter or Space to run.
- Enter or Space on an empty line repeats the last command; Esc cancels.
- Prompts show options in brackets, e.g. `Specify next point or [Undo/Close]:`. Typing the capital letter picks the option.
- Point input: click, absolute `10,20`, relative `@10,0`, polar `@50<45`, or direct distance (move the mouse in a direction and type `50`).
- Each command is a state machine implementing one `Command` trait: `on_point`, `on_text`, `on_option`, `preview`, `finish`, `cancel`.

### First 20 commands

| Command | Alias | What it must do in v0.1 |
| --- | --- | --- |
| LINE | L | Chained segments; Undo and Close options |
| PLINE | PL | Lightweight polyline; Arc and Line modes, width |
| CIRCLE | C | Center-radius, center-diameter, 2P, 3P |
| ARC | A | 3-point, start-center-end |
| RECTANG | REC | Two corners; result is a closed polyline |
| TEXT | T | Single-line text: insertion point, height, rotation |
| MOVE | M | Base point to second point, live preview |
| COPY | CO | Like MOVE, multiple copies until Enter |
| ROTATE | RO | Base point and angle, Reference option |
| SCALE | SC | Base point and factor, Reference option |
| MIRROR | MI | Two-point mirror line, keep or erase source |
| OFFSET | O | Distance and side; works on lines, arcs, circles, polylines |
| TRIM | TR | Pick objects to cut back to nearest boundaries |
| EXTEND | EX | Extend to nearest boundary |
| FILLET | F | Radius, including 0 for a sharp corner |
| ERASE | E | Delete the selection |
| EXPLODE | X | Polylines to segments, blocks to their contents |
| DIMLINEAR | DLI | Horizontal or vertical dimension with arrows and text |
| LAYER | LA | Panel to create, rename, colour, freeze, lock, set current |
| ZOOM | Z | Extents, Window, Previous |

UNDO (U) and REDO run outside this list: every command is one undo step, including its preview cancellations.

### Precision aids

- Object snap (F3): endpoint, midpoint, center, intersection, perpendicular, tangent, nearest; snap marker shapes shown at the cursor.
- Ortho (F8): lock to 0/90°. Polar tracking (F10) at 15° increments.
- Grid display (F7) and grid snap (F9).

### Selection

- Click picks one entity; Shift+click removes.
- Drag left-to-right = window (fully inside); right-to-left = crossing (touching), drawn with different box styles.
- Commands that need a selection accept a pre-selection or prompt `Select objects:`.

### Output

- Save and Save As to DXF (2018 default, R12 option).
- Plot to PDF and SVG: choose window or extents, paper size, scale (e.g. 1:100), and line weights by layer.

### Suggested crates

`egui` + `wgpu` (UI and canvas), `lyon` (path tessellation), `kurbo` (curve geometry), `rstar` (R-tree), `cosmic-text` or `rustybuzz` (text shaping), `printpdf` or `krilla` (PDF), `svg` (SVG export). Pin exact versions and check each licence is MIT/Apache-compatible.

## AI-assisted tasks

Planned after v0.1, built on the command engine (M5–M7) and visual diff (M4b). Every AI-assisted task follows the core principle and the same flow:

1. **Request.** The user describes the outcome in one clear request (e.g. "bring this drawing up to our layer standard").
2. **Plan shown as real commands.** The AI proposes a plan as a list of the app's own commands (the same commands a user could type), with the objects each one affects. Nothing has changed yet.
3. **Preview in visual diff.** The planned result is shown against the current drawing using the M4b visual diff: added, deleted and changed entities highlighted, with the change list.
4. **User accepts all, some or none.** The user can apply the whole plan, pick individual changes, or reject it. Nothing is applied without this step.
5. **Independent rule-based check.** After applying, a deterministic validator (geometry, standards and consistency rules written as ordinary tested code) checks the result and reports any problems. The AI never checks its own work.
6. **Saved log, undoable as one step.** The request, plan, accepted changes and check results are saved in a log alongside the drawing, and the whole task is one undo step.

**First target tasks**

- **Standards check and fix.** Compare a drawing against a standard (layer names, colours, linetypes, text styles, dimension styles) and propose fixes for every deviation.
- **Revision comparison with notes.** Compare two revisions and write plain-language notes for each change, each note linked to the entities it describes.
- **Quantity takeoff.** Count and measure (lengths, areas, block counts), with every number in the result traceable to the objects it came from.
- **Batch title-block update and plotting.** Update title-block attributes (revision, date, sheet numbers) across many drawings, then plot them, previewing every change first.

## Milestones

Nine milestones, each small enough for one focused Claude Code task. Don't start the next until the current one passes its acceptance criteria. M1–M4b lead to a public preview release; M5–M7 lead to v0.1, released the same way.

1. **M1 — Tokenizer and model.** Workspace set up; `lightable-dxf` reads group-code pairs and parses `HEADER`, `TABLES` (layers only) and `LINE`/`CIRCLE`/`ARC`.
   - Accept: 10 simple corpus files load with zero panics; unit tests per entity; CI runs `cargo test`, `clippy`, `fmt`.
2. **M2 — Full v0.1 reader.** All v0.1 entities, blocks, text encodings, `Unknown` preservation, warnings.
   - Accept: all 50+ corpus files load; fuzzing runs 1 hour with no crash; 50 MB file loads in under 2 s.
3. **M3 — Writer and round-trip.** R12 and 2018 output.
   - Accept: round-trip test passes for the whole corpus; written files open cleanly in two other CAD programs.
4. **M4 — Viewer.** `lightable` opens a DXF and shows it with pan, zoom, layers on/off, and golden SVG snapshots.
   - Accept: 100k-entity file at 60 fps; snapshots match for the corpus.
5. **M4b — Visual diff.** Open two DXF files and highlight added (green), deleted (red) and changed (yellow) entities. Entities are matched by handle first, and by geometry (same type, layer and coordinates within a tolerance) when handles differ, e.g. after a save in another program. Show a summary list of the changes and a "Next change" button that zooms to each one in turn. The comparison logic lives in a UI-free crate function with tests; the viewer only displays its result.
   - Accept: tests on pairs of corpus files with known edits (added, deleted, moved, restyled entities, and pairs whose handles differ); a demo comparing two revisions of a floor plan.
6. **Preview release.** First public release, built from M1–M4b:
   - Pick the product name first (see the rules above).
   - Tagged GitHub release with Windows, macOS and Linux installers built by CI from the tag.
   - A WebAssembly build of the viewer and visual diff, hosted on GitHub Pages.
   - README with screenshots.
   - Issue templates for bug reports and feature requests.
   - An in-app "Send feedback" link that opens the issue tracker in the browser; nothing is sent automatically.
   - No telemetry.
   - Accept: installers from the tagged CI run install and open a corpus file on all three platforms; the GitHub Pages build opens and diffs two corpus files in a browser; issue templates and the feedback link work; a check confirms the app makes no network requests on its own.
7. **M5 — Command engine and drawing commands.** Command line, point input formats, LINE, PLINE, CIRCLE, ARC, RECTANG, TEXT, ERASE, undo/redo, object snap, ortho.
   - Accept: a scripted test draws a floor-plan outline using only typed commands and saves a valid DXF.
8. **M6 — Editing commands.** MOVE, COPY, ROTATE, SCALE, MIRROR, OFFSET, TRIM, EXTEND, FILLET, EXPLODE, selection windows.
   - Accept: each command has geometry tests including edge cases (parallel lines, zero radius, arcs crossing 0°).
9. **M7 — Dimensions, layers, plot, v0.1 release.** DIMLINEAR, LAYER panel, PDF/SVG plot, then release v0.1 the same way as the preview: CI-built installers for Windows, macOS and Linux from the tag, an updated WebAssembly build on GitHub Pages, README with screenshots, issue templates and the "Send feedback" link, and no telemetry.
   - Accept: a sample drawing plots to a correctly scaled A3 PDF at 1:100; README with screenshots and a side-by-side demo video; v0.1 tagged on GitHub with installers built by CI for all three platforms and the GitHub Pages build updated.

## Working with Claude Code

Put this spec in the repo as `docs/SPEC.md` and the rules below in `CLAUDE.md`, so every session starts with the same context.

**Rules for `CLAUDE.md`**

- Core principle (as at the top of this spec): one clear request for a professional task, with transparency, control and independent validation.
- Every automated or AI feature must show its plan, preview changes before applying, let the user approve or reject, be checked by an independent rule-based validator, and keep a log.
- Read `docs/SPEC.md` before any task. Work on one milestone at a time.
- Clean-room only: use public DXF documentation and test files. Never paste or imitate another CAD vendor's source code, icons or branding.
- Never panic on user input; return errors or warnings.
- Unknown DXF data must survive a round-trip unchanged.
- Every new entity or command ships with tests. Run `cargo test`, `cargo clippy -- -D warnings` and `cargo fmt --check` before finishing.
- Pin exact dependency versions; only MIT/Apache-2.0/BSD/zlib licences.
- Keep a `CHANGELOG.md` entry per milestone.

**Starter prompt for M1**

```text
Read docs/SPEC.md and CLAUDE.md. Set up the Cargo workspace described in the Architecture section. Then implement milestone M1 only: the DXF tokenizer, the Drawing/Header/Tables/Entity model, and parsing of LINE, CIRCLE and ARC. Add unit tests and a GitHub Actions CI workflow. Stop when M1's acceptance criteria pass and summarise what's left for M2.
```

**Prompt pattern for every later milestone**

```text
Read docs/SPEC.md and CLAUDE.md. Implement milestone M<N> only. Acceptance criteria: <paste from the spec>. Write tests first for the trickiest cases, then the code. Run the full test suite and fix failures before finishing. List any spec questions you had to decide yourself.
```

**Tips**

- Ask Claude Code to write a tiny DXF by hand for each new entity before parsing it; small fixtures make bugs obvious.
- For geometry commands (TRIM, OFFSET, FILLET), ask for the maths as a pure function in a `geom` module first, with tests, before wiring the UI.
- When a corpus file renders wrong, give Claude Code the file, the snapshot diff, and a screenshot from another viewer.
