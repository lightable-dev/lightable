"""Imports DXF files into FreeCAD, one after another in one process (FreeCAD
starts slowly).

Run with freecadcmd:
    freecadcmd tools/freecad_check.py --pass LIST_FILE

LIST_FILE has one DXF path per line. For each file this prints a line
"@@BEGIN <path>", whatever FreeCAD's importer prints, then
"@@END <json>" with {"objects": n, "exception": text or null}.
tools/validate_written.py reads the importer's messages between the markers.
"""

import json
import sys

import FreeCAD
import importDXF

args = sys.argv[sys.argv.index("--pass") + 1:]
paths = [line.strip() for line in open(args[0], encoding="utf-8") if line.strip()]

for path in paths:
    print(f"@@BEGIN {path}", flush=True)
    objects, exception = 0, None
    try:
        doc = FreeCAD.newDocument("check")
        importDXF.insert(path, doc.Name)
        objects = len(doc.Objects)
        FreeCAD.closeDocument(doc.Name)
    except Exception as exc:  # noqa: BLE001 - every failure is reported
        exception = f"{type(exc).__name__}: {exc}"
    sys.stdout.flush()
    print("@@END " + json.dumps({"objects": objects, "exception": exception}), flush=True)
