"""Re-saves a DXF through FreeCAD: import into a new document, export again.

Run with freecadcmd, which passes script arguments after --pass:
    freecadcmd tools/freecad_resave.py --pass IN.dxf OUT.dxf

Called by tools/convert_corpus.sh. FreeCAD 1.1's exporter writes R14
(AC1014); its DxfVersionOut preference had no effect when run headless.
"""

import sys

import FreeCAD
import importDXF

args = sys.argv[sys.argv.index("--pass") + 1:]
src, dst = args[0], args[1]

doc = FreeCAD.newDocument("resave")
importDXF.insert(src, doc.Name)
doc.recompute()
objects = [o for o in doc.Objects
           if hasattr(o, "Shape") or o.TypeId.startswith("App::Annotation")]
print(f"imported {len(objects)} objects from {src}")
importDXF.export(objects, dst)
FreeCAD.closeDocument(doc.Name)
