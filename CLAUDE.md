# CLAUDE.md

Project: open-source 2D CAD app in Rust (DXF library + drafting app). Full spec: `docs/SPEC.md`.

## Rules

- Core principle: take a professional task that currently needs expertise, multiple operations and manual checking, and make the outcome achievable through one clear request, while keeping transparency, control and independent validation.
- Every automated or AI feature must show its plan, preview changes before applying, let the user approve or reject, be checked by an independent rule-based validator, and keep a log.
- Read `docs/SPEC.md` and `docs/DECISIONS.md` before any task; don't contradict a recorded decision without saying so, and add new ones there. Work on one milestone at a time; don't start the next until the current one passes its acceptance criteria.
- Clean-room only: use public DXF documentation and test files. Never paste or imitate another CAD vendor's source code, icons or branding, and never use the names of existing CAD applications in the product.
- Never panic on user input; return errors or warnings.
- Unknown DXF data must survive a round-trip unchanged.
- Use `f64` for all coordinates.
- Every new entity or command ships with tests. Run `cargo test`, `cargo clippy -- -D warnings` and `cargo fmt --check` before finishing.
- Geometry maths goes in `lightable-geom` as pure, tested functions before it is wired into commands or UI.
- Pin exact dependency versions; only MIT, Apache-2.0, BSD or zlib licences. Bundled fonts may also use the Ubuntu Font Licence 1.0 or Hack's licence (MIT with the Bitstream Vera licence).
- Dev-only tools (fuzzers, benchmarks) may use other permissive licences such as NCSA, as long as they never ship.
- Add a `CHANGELOG.md` entry per milestone.
- When you have to decide something the spec doesn't cover, record it in `docs/DECISIONS.md` and note it in your summary.
