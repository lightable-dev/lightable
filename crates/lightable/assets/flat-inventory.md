# flat: expected-change inventory

Flat 3.02 floor plan, Rev A -> Rev B: the door block DOOR-800 is redrawn with a 40 mm thick leaf (one edit to the block definition, shown by all 7 doors), and the partition between the two bedrooms moves 300 mm to the right (its two wall lines).

Written by tools/gen_usecases.py from the edits it makes, before the files are compared.

## Design changes (4)

| # | Type | Layer | Expected words |
|---|---|---|---|
| E1 | LINE |  | In block DOOR-800; deleted |
| E2 | LWPOLYLINE |  | In block DOOR-800; added |
| E3 | LINE | WALLS | moved 300 mm right |
| E4 | LINE | WALLS | moved 300 mm right |

## Not changes

- the door block is shown by 7 references (INSERT); each shows the edit
