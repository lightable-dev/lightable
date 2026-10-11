# Bracket LT-0042, Rev B → Rev C: expected-change inventory

Written before the drawings were compared, from the engineering change
itself. `bracket-rev-c-inventory.json` holds the same list in a form the
tests check against the comparison (`crates/lightable-diff/tests/examples.rs`).

## The part

Steel mounting bracket, 160 × 100 mm plate, 10 mm thick, S235JR
(EN 10025-2), bolted to a mating frame with four bolts through clearance
holes (no tapped holes). General tolerances ISO 2768-m.

| | Rev B | Rev C |
|---|---|---|
| Bolts | 4 × M12, ISO 4017, property class 8.8 | 4 × M16, ISO 4017, property class 8.8 |
| Clearance holes | Ø13.5 (ISO 273, medium series, M12) | Ø17.5 (ISO 273, medium series, M16) |
| Edge distance (hole centre to plate edge) | 20 mm | 25 mm |
| Hole pitch | 120 × 60 mm | 110 × 50 mm |

## The engineering change (ECN-017)

**Reason:** higher bolt load at the mating frame; the bolts go from M12 to
M16.

**Consequences, worked out from references:**

1. **Clearance holes Ø13.5 → Ø17.5.** ISO 273 medium-series clearance for
   M16 is 17.5 mm (M12: 13.5 mm).
2. **Holes move inward by 5 mm in X and Y.** EN 1993-1-8 Table 3.3 asks
   for an edge distance of at least 1.2 d₀. With d₀ = 17.5 mm that is
   21 mm, so the Rev B edge distance of 20 mm is no longer enough; Rev C
   uses 25 mm. The hole pattern becomes 110 × 50 mm. Spacing stays above
   the minimum of 2.4 d₀ = 42 mm.
3. **The hole callout and the dimensions follow:** "4x Ø13.5 THRU" →
   "4x Ø17.5 THRU"; the edge-distance dimensions 20 → 25 (two), the pitch
   dimensions 120 → 110 and 60 → 50.
4. **The bolt note changes:** "BOLTS: 4x M12 ISO 4017 - 8.8" →
   "BOLTS: 4x M16 ISO 4017 - 8.8".

**Unchanged:** plate outline and overall dimensions (160, 100), thickness,
material, tolerance notes.

**Revision markup added by the drafter:** title block revision letter
B → C; a revision-table row for C; a revision cloud and a revision
triangle "C" — drawn **only around the bolt note**. The other consequences
(holes, hole positions, callout, dimensions) are not clouded. That is the
point of the example: the cloud suggests one small change, while the
drawing a supplier would build from changed in more places, including the
hole pattern a mating part has to match.

## Expected differences in the drawing data

Design content (what a supplier builds from):

| # | Item | Expected change |
|---|---|---|
| E1 | Bolt note (text) | M12 → M16 in "BOLTS: 4x M… ISO 4017 - 8.8" — clouded |
| E2 | Hole, lower left (circle) | Ø13.5 → Ø17.5; centre moves 5 mm right and 5 mm up |
| E3 | Hole, lower right (circle) | Ø13.5 → Ø17.5; centre moves 5 mm left and 5 mm up |
| E4 | Hole, upper right (circle) | Ø13.5 → Ø17.5; centre moves 5 mm left and 5 mm down |
| E5 | Hole, upper left (circle) | Ø13.5 → Ø17.5; centre moves 5 mm right and 5 mm down |
| E6 | Hole callout (text) | "4x Ø13.5 THRU" → "4x Ø17.5 THRU" |
| E7 | Dimension, top edge distance | 20 → 25 |
| E8 | Dimension, top hole pitch | 120 → 110 |
| E9 | Dimension, right edge distance | 20 → 25 |
| E10 | Dimension, right hole pitch | 60 → 50 |

Revision markup (records the revision; not part of the part):

| # | Item | Expected change |
|---|---|---|
| M1 | Title block revision letter | REV B → REV C |
| M2 | Revision table | row "C" and its description added |
| M3 | Revision cloud | added around the bolt note |
| M4 | Revision triangle | triangle and its letter "C" added next to the cloud |

## What a comparison of the files will also show

- Each dimension's drawn picture lives in an anonymous block that every
  CAD program regenerates; the comparison does not compare those blocks
  (it says so under "Not compared yet") and compares the dimension
  entities themselves instead.
- One engineering change (ECN-017) shows up as ten design-content
  differences and 8 markup items (drawing objects). The counts are not meant to
  match: one intention, many consequences.
