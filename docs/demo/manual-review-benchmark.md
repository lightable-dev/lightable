# Benchmark: the bracket example against manual review

The question: a reviewer checks Rev C of bracket LT-0042 before it goes to
the supplier. What does each method show, and what does it take?

The manual baselines use the same renderings as the app's snapshots
(`tests/snapshots/examples/bracket-rev-*.svg`), shown the way a reviewer
looks at two PDFs: `docs/demo/manual-review.html` (serve the repository
root, e.g. `python3 tools/web_test_server.py`, and open
`/docs/demo/manual-review.html`).

The expected changes were written down before the comparison
(`crates/lightable/assets/bracket-rev-c-inventory.md`): 10 design-content
differences (E1–E10) from one engineering change, and 4 markup items
(M1–M4).

## Method 1: two PDFs side by side

What draws the eye: the revision cloud and triangle around the bolt note,
the new revision-table row "BOLTS M12 TO M16 (ECN-017)", and "REV C". All
three point to one change.

| Item | Visible side by side? |
|---|---|
| E1 bolt note M12 → M16 | Yes: clouded. |
| E2–E5 holes Ø13.5 → Ø17.5, moved 5 mm inward | Only by comparing circles carefully: the circles are about 30 % larger and sit 5 mm further in, at a scale where 5 mm is a few pixels. |
| E6 callout Ø13.5 → Ø17.5 | Only by reading the text in both. |
| E7–E10 dimensions 20 → 25, 120 → 110, 20 → 25, 60 → 50 | Only by reading every dimension value in both. |
| M1–M4 markup | Yes. |

Effort: read every text and dimension twice and compare. A reviewer who
trusts the cloud checks E1 and the markup and stops; nine design changes,
including the hole pattern the mating part has to match, go unchecked.

## Method 2: pixel overlay of the two PDFs

Everything that drew differently shows as coloured pixels on black.

| Item | Visible in the overlay? |
|---|---|
| E2–E5 holes | Yes: two circles each, clearly. |
| E7–E10 dimensions | Partly: the moved extension and dimension lines stand out; the value change (20 → 25, 120 → 110) is a speck where the digits differ. |
| E1, E6 texts | A speck where "12" / "16" or "13.5" / "17.5" differ; easy to miss at page scale. |
| M1–M4 markup | Yes. |

The overlay finds *where* something changed, not *what*: the reviewer still
has to zoom into each spot and read both revisions. It depends on both
PDFs being plotted at the same scale and position; a shifted plot or a
different sheet makes everything differ. Changes that do not move pixels
(a layer, a colour that prints the same, a block definition used nowhere
visible) do not show.

## Method 3: Lightable

| Item | Lightable's result |
|---|---|
| E1 | "Text changed from BOLTS: 4x M12 ISO 4017 - 8.8 to BOLTS: 4x M16 ISO 4017 - 8.8" |
| E2–E5 | "Circle diameter changed from 13.5 mm to 17.5 mm; center moved 5 mm right and 5 mm up" (and left/down variants), one per hole |
| E6 | "Text changed from 4x Ø13.5 THRU to 4x Ø17.5 THRU" |
| E7, E9 | "Dimension changed from 20 mm to 25 mm" |
| E8 | "Dimension changed from 120 mm to 110 mm" |
| E10 | "Dimension changed from 60 mm to 50 mm" |
| M1 | "Text changed from REV B to REV C" (revision markup) |
| M2 | "Polyline added", "Line added", "Text “C” added", "Text “BOLTS M12 TO M16 (ECN-017)” added" (revision markup) |
| M3 | "Revision cloud added" (revision markup) |
| M4 | "Polyline added", "Text “C” added" (revision markup) |

Summary line: "10 changes found between Rev B and Rev C", beside it "plus revision
markup (8 items, including 1 revision cloud)"; the markup is listed separately. Every expected item is found, nothing
is reported that is not in the inventory (test
`bracket_rev_c_matches_the_expected_change_inventory`). Each line zooms to
the change on click. The PNG and JSON reports carry the same list.

It compares the DXF files, not plots, so plot scale and sheet position do
not matter, and non-visual changes (layers, colours, block definitions,
units) are reported too. It does not compare what "Not compared yet" lists
(text and dimension styles, other tables and settings, objects, the
dimension pictures in anonymous blocks).

## Narration check

| Demo line | What the product does | True? |
|---|---|---|
| "Before and after: Rev B on the left, Rev C on the right." | Side by side view with the example labels. | Yes |
| "The revision cloud marks 1 change." | The demo counts design changes whose centre lies inside the added cloud. The product itself does not check clouds. | Yes for this example (tested); demo narration only |
| "Lightable found 10 changes." | `design_changes().count()`; markup is listed separately. | Yes (live count) |
| Three headlines | The real headlines of three changes the demo clicks. | Yes (live) |
| "Details: the technical data, and what was not compared." | The Details toggle shows exact values, entity type, handle, layer, and "Not compared yet". | Yes |
| "Download the review report as PNG or JSON." | Both exports exist under Details; the demo shows the PNG layout without saving. | Yes |
| "Lightable shows what changed. Whether it is right is still your call." | No design, safety or manufacturing checks exist. | Yes |
| "Your files never leave your computer." | The web page has no upload path; its Content-Security-Policy allows requests only to its own origin, and the browser tests record no request carrying file data. | Yes |
