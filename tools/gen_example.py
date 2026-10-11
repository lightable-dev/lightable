"""The built-in example: two revisions of a small apartment floor plan.

Usage: tools/.venv/bin/python tools/gen_example.py crates/lightable/assets

Writes example-rev-a.dxf and example-rev-b.dxf (millimetres). Everything
in them is generated here with ezdxf (MIT), so the files are ours to ship
under the project's licence. Revision B shows each kind of change the
diff reports: a moved door, a removed wall, a new kitchen island, a
renamed room, a changed floor hatch, an edited block definition (the
chair, used by several references), a layer colour change and a new
dimension.
"""

import sys
from pathlib import Path

import ezdxf
from ezdxf.enums import TextEntityAlignment

OUT = Path(sys.argv[1])
OUT.mkdir(parents=True, exist_ok=True)


def build():
    """Revision A; returns the document and the entities B edits."""
    doc = ezdxf.new("R2018", setup=True)
    doc.header["$INSUNITS"] = 4
    for name, color in [("WALLS", 7), ("DOORS", 3), ("WINDOWS", 4), ("FURNITURE", 8), ("TEXT", 2),
                        ("FLOOR", 9), ("DIMENSIONS", 1)]:
        doc.layers.add(name, color=color)
    chair = doc.blocks.new("CHAIR")
    chair.add_lwpolyline([(-220, -220), (220, -220), (220, 220), (-220, 220)], close=True)
    chair.add_line((-220, 160), (220, 160))
    doc.blocks.new("TABLE").add_circle((0, 0), 500)
    msp = doc.modelspace()
    e = {}
    walls = {"layer": "WALLS"}
    # Outer walls (10 m × 7 m), inner and outer faces.
    msp.add_lwpolyline([(0, 0), (10000, 0), (10000, 7000), (0, 7000)], close=True, dxfattribs=walls)
    msp.add_lwpolyline([(200, 200), (9800, 200), (9800, 6800), (200, 6800)], close=True, dxfattribs=walls)
    msp.add_line((6000, 200), (6000, 3000), dxfattribs=walls)
    msp.add_line((6000, 3900), (6000, 6800), dxfattribs=walls)
    msp.add_line((6000, 4500), (9800, 4500), dxfattribs=walls)
    e["kitchen_wall"] = msp.add_line((200, 4200), (2600, 4200), dxfattribs=walls)
    doors = {"layer": "DOORS"}
    msp.add_arc((4000, 200), 900, 0, 90, dxfattribs=doors)
    msp.add_line((4000, 200), (4000, 1100), dxfattribs=doors)
    e["bed_door_arc"] = msp.add_arc((6000, 3000), 900, 90, 180, dxfattribs=doors)
    e["bed_door_leaf"] = msp.add_line((6000, 3000), (5100, 3000), dxfattribs=doors)
    for x in (1000, 3000, 7500):
        msp.add_lwpolyline([(x, 6800), (x + 1200, 6800), (x + 1200, 7000), (x, 7000)], close=True,
                           dxfattribs={"layer": "WINDOWS"})
    msp.add_blockref("TABLE", (3000, 2500), dxfattribs={"layer": "FURNITURE"})
    for x, y, rot in [(3000, 1750, 0), (3000, 3250, 180), (2250, 2500, 270), (3750, 2500, 90)]:
        msp.add_blockref("CHAIR", (x, y), dxfattribs={"layer": "FURNITURE", "rotation": rot})
    e["bath_floor"] = h = msp.add_hatch(color=256, dxfattribs={"layer": "FLOOR"})
    h.paths.add_polyline_path([(6200, 4700), (9600, 4700), (9600, 6600), (6200, 6600)], is_closed=True)

    def label(text, x, y):
        t = msp.add_text(text, height=200, dxfattribs={"layer": "TEXT"})
        t.set_placement((x, y), align=TextEntityAlignment.MIDDLE_CENTER)
        return t

    label("LIVING / DINING", 3000, 3800)
    e["kitchen_label"] = label("KITCHEN", 1400, 4600)
    e["bedroom_label"] = label("BEDROOM", 7900, 2400)
    label("BATH", 7900, 5600)
    msp.add_linear_dim(base=(0, -600), p1=(0, 0), p2=(10000, 0), dimstyle="EZDXF",
                       dxfattribs={"layer": "DIMENSIONS"}).render()
    return doc, e


def revise(doc, e):
    """Revision B: the same document edited, so unchanged entities keep
    their handles (as when a CAD program saves a new revision)."""
    msp = doc.modelspace()
    doc.layers.get("FURNITURE").color = 6
    chair = doc.blocks.get("CHAIR")
    chair.add_lwpolyline([(-260, -180), (-220, -180), (-220, 180), (-260, 180)])
    chair.add_lwpolyline([(260, -180), (220, -180), (220, 180), (260, 180)])
    msp.delete_entity(e["kitchen_wall"])  # open-plan kitchen
    e["bed_door_arc"].translate(0, 600, 0)
    e["bed_door_leaf"].translate(0, 600, 0)
    msp.add_lwpolyline([(900, 5000), (2700, 5000), (2700, 5800), (900, 5800)], close=True,
                       dxfattribs={"layer": "FURNITURE"})  # kitchen island
    e["bath_floor"].set_pattern_fill("ANSI37", scale=20, color=256)
    e["kitchen_label"].translate(0, 1600, 0)
    e["bedroom_label"].dxf.text = "MASTER BEDROOM"
    msp.add_linear_dim(base=(10600, 0), p1=(10000, 0), p2=(10000, 7000), angle=90, dimstyle="EZDXF",
                       dxfattribs={"layer": "DIMENSIONS"}).render()


def main():
    doc, e = build()
    doc.saveas(OUT / "example-rev-a.dxf")
    revise(doc, e)
    doc.saveas(OUT / "example-rev-b.dxf")


if __name__ == "__main__":
    main()
