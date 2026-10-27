# plate: expected-change inventory

Locating plate LT-0115, Rev B -> Rev C: the second dowel hole moves 0.5 mm in X to suit the mating part, its position dimension follows (80 -> 80.5), and the hole-position tolerance tightens from ±0.1 to ±0.05. Nothing is clouded.

Written by tools/gen_usecases.py from the edits it makes, before the files are compared.

## Design changes (3)

| # | Type | Layer | Expected words |
|---|---|---|---|
| E1 | CIRCLE | HOLES | moved 0.5 mm right |
| E2 | DIMENSION | DIMS | Dimension changed from 80 mm to 80.5 mm |
| E3 | TEXT | NOTES | Text changed from HOLE POSITIONS ±0.1 to HOLE POSITIONS ±0.05 |

## Revision markup

| # | What |
|---|---|
| M1 | Text changed from REV B to REV C |
| M2 | revision-table row C |
