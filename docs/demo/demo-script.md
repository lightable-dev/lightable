# The 30-second demo (`?demo`, `--demo`)

Situation: a manufacturing engineer checks Rev C of a steel bracket drawing
before it goes to the supplier. The revision cloud suggests a small change;
the drawing changed in more places.

First screen (before the demo starts, and for any pair of files):

> **10 changes found between Rev B and Rev C** · plus revision markup (8 items, including 1 revision cloud)
> *Example: the drafter clouded only the bolt note.*

The first line comes from the comparison for any files (the cloud part is
left out when no revision cloud changed). The italic line belongs to the
built-in bracket example only: it describes how the example was drawn and
is never shown for your own files. The 8 markup items are drawing objects:
the revision letter, the revision-table row (a polyline, a line and two
texts), the cloud, and the revision triangle with its letter.

Every number and headline on screen comes from the comparison of the two
DXF files at the moment the demo runs (`crates/lightable/src/app.rs`,
`run_demo`). Nothing is typed in by hand except the connecting words.

Each step stays on screen for its caption's reading time: 3 words a
second plus one second for the zoom to settle, at least 3 seconds
(`caption_seconds` in `app.rs`). The demo starts one second after the app
is ready; with the bracket it runs about 37 seconds, then holds the end
card.

| Start | Length | On screen | Caption | Where the words come from |
|---|---|---|---|---|
| 0 s | 4.0 s | Rev B left, Rev C right, whole sheet | "Before and after: Rev B left, Rev C right." | Example labels |
| 4.0 s | 3.0 s | Zoom to the revision cloud | "The revision cloud marks 1 change." | Count of design changes whose centre lies inside the added revision cloud (demo narration only; see "Limits") |
| 7.0 s | 3.0 s | Overlay, whole sheet | "Lightable found 10 changes." | `Diff::design_changes().count()` (revision markup not counted) |
| 10.0 s | 7.0 s | A hole, zoomed and framed | "Circle diameter changed from 13.5 mm to 17.5 mm; center moved 5 mm right and 5 mm up" | Headline of the first changed circle outside the cloud |
| 17.0 s | 3.7 s | A dimension | "Dimension changed from 20 mm to 25 mm" | Headline of the first changed dimension outside the cloud |
| 20.7 s | 4.3 s | The hole callout | "Text changed from 4x Ø13.5 THRU to 4x Ø17.5 THRU" | Headline of the first changed text outside the cloud |
| 25.0 s | 3.7 s | Details on, list scrolled to "Not compared yet" | "Details: exact values, and what is not compared." | The Details layer |
| 28.7 s | 3.7 s | Report layout | "Download the review report as PNG or JSON." | The PNG report layout (shown, not saved, so no dialog interrupts a recording) |
| 32.3 s | 5.0 s | Overlay | "Lightable shows what changed. Whether it is right is still your call." | Scope statement |
| 37.3 s | holds | End card | "Free. Open source. / Your files never leave your computer. / lightable.dev" | |

The selected change gets a white frame on a dark halo, 2–6 points thick
depending on the screen, so it shows in a small video. Captions scale with
the shorter side of the screen (about 41 px in a 1080p frame) and wrap.

## Recording formats

| Format | Address | Record at |
|---|---|---|
| 16:9 (LinkedIn, YouTube) | `?demo&format=landscape` | 1920 × 1080 |
| 9:16 (Shorts, Reels) | `?demo&format=vertical` | 1080 × 1920 |

`format` fixes the drawing canvas to that shape, centred on black, so any
window size records the right frame; the app lays itself out for it (in
9:16 the change list goes under the drawing). For a pixel-exact capture,
use a browser window or device emulation of 960 × 540 (or 540 × 960) CSS
pixels at a device pixel ratio of 2. Frames can be captured that way with
`tools/web_browser_test.mjs --viewport 960,540,2 --at …`.

The three clicks are chosen from the real results by `Compare::demo_picks`:
one change of each kind (circle, dimension, text) that the cloud does not
surround. The test `the_demo_narrates_the_bracket_from_real_results`
(`crates/lightable/src/compare.rs`) checks the captions against the
comparison; `the_bracket_cloud_surrounds_exactly_one_design_change`
(`crates/lightable-diff/tests/examples.rs`) checks the cloud sentence.

## What the demo does not claim

- Lightable does not check whether a revision cloud covers every change.
  The cloud sentence is narration about this example, computed for the
  demo only.
- It does not say whether a drawing is ready for manufacturing, whether a
  design is safe, or whether a change is correct.
- It compares drawing data (entities, their values, layers, blocks, units).
  It does not understand the part. "Not compared yet" lists what it skips.

## Other examples

`?demo&example=plan` plays the same script on the floor plan: there is no
revision cloud, so the second caption says so, and the clicks are three of the plan's
changes (a circle, dimension or text first where there is one).
