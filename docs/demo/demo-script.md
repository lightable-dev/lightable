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

| Time | On screen | Caption | Where the words come from |
|---|---|---|---|
| 0 s | Rev B left, Rev C right, whole sheet | "Before and after: Rev B on the left, Rev C on the right." | Example labels |
| 3.5 s | Zoom to the revision cloud | "The revision cloud marks 1 change." | Count of design changes whose centre lies inside the added revision cloud (demo narration only; see "Limits") |
| 7 s | Overlay, whole sheet | "Lightable found 10 changes." | `Diff::design_changes().count()` (revision markup not counted) |
| 10.5 s | Click on a hole: zoom, highlighted | "Circle diameter changed from 13.5 mm to 17.5 mm; center moved 5 mm right and 5 mm up" | Headline of the first changed circle outside the cloud |
| 13.5 s | Click on a dimension | "Dimension changed from 20 mm to 25 mm" | Headline of the first changed dimension outside the cloud |
| 16.5 s | Click on the hole callout | "Text changed from 4x Ø13.5 THRU to 4x Ø17.5 THRU" | Headline of the first changed text outside the cloud |
| 19.5 s | Details on: exact values, "Not compared yet" | "Details: the technical data, and what was not compared." | The Details layer |
| 23 s | Report layout | "Download the review report as PNG or JSON." | The PNG report layout (shown, not saved, so no dialog interrupts a recording) |
| 26.5 s | Overlay | "Lightable shows what changed. Whether it is right is still your call." | Scope statement |
| 30 s | End card | "Free. Open source. / Your files never leave your computer. / lightable.dev" | |

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
