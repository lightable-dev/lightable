# depot: expected-change inventory

Distribution centre layout, Rev A -> Rev B (23182 objects in Rev B): one location label renamed, one bay frame moved 300 mm, one column added.

Written by tools/gen_usecases.py from the edits it makes, before the files are compared.

## Design changes (3)

| # | Type | Layer | Expected words |
|---|---|---|---|
| E1 | TEXT | LABELS | Text changed from R1-065 to QC-01 |
| E2 | LWPOLYLINE | RACKS | moved 300 mm right |
| E3 | CIRCLE | COLUMNS | added |

## Not changes

- 23182 objects in Rev B (model space entities); the files are written by crates/lightable/src/depot.rs (deterministic) and not committed
