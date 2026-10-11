"""The built-in examples (millimetres, $INSUNITS = 4).

Usage: tools/.venv/bin/python tools/gen_example.py crates/lightable/assets

- bracket-rev-b.dxf / bracket-rev-c.dxf (the default example): a steel
  mounting bracket drawing (160 x 100 x 10 mm plate, four bolts through
  clearance holes). Rev C is one engineering change, bolts M12 -> M16,
  with its consequences: larger clearance holes, holes moved for edge
  distance, the callout and dimensions updated. The drafter clouds only
  the bolt note. The expected changes were written down first, in
  bracket-rev-c-inventory.md / .json.
- plan-rev-a.dxf / plan-rev-b.dxf: two revisions of a small apartment
  plan showing each kind of change the diff reports: a moved door, a
  removed wall, a new kitchen island, a renamed room, a changed floor
  hatch, an edited block definition (the chair, used by several
  references), a layer colour change and a new dimension.

Everything in them is generated here with ezdxf (MIT), so the files are
ours to ship under the project's licence. Each later revision is the same
document edited, so unchanged entities keep their handles, as when a CAD
program saves a new revision.
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


def bracket():
    """Rev B of the bracket; returns the document and what Rev C edits.

    A 160 x 100 mm plate, 10 mm S235JR, four M12 bolts through Ø13.5
    clearance holes (ISO 273 medium series) at 20 mm edge distance
    (EN 1993-1-8 Table 3.3: at least 1.2 d0 = 16.2 mm)."""
    doc = ezdxf.new("R2018", setup=True)
    doc.header["$INSUNITS"] = 4
    for name, color in [("OUTLINE", 7), ("HOLES", 4), ("NOTES", 2), ("DIMS", 3), ("BORDER", 8),
                        ("TITLE", 8), ("REVISION", 1)]:
        doc.layers.add(name, color=color)
    msp = doc.modelspace()
    e = {}
    LEFT = TextEntityAlignment.LEFT
    MID = TextEntityAlignment.MIDDLE_CENTER

    def text(s, x, y, h, layer, align=LEFT):
        t = msp.add_text(s, height=h, dxfattribs={"layer": layer})
        t.set_placement((x, y), align=align)
        return t

    def line(a, b, layer):
        return msp.add_line(a, b, dxfattribs={"layer": layer})

    # Sheet border, title block and revision table.
    msp.add_lwpolyline([(0, 0), (297, 0), (297, 210), (0, 210)], close=True, dxfattribs={"layer": "BORDER"})
    msp.add_lwpolyline([(187, 5), (292, 5), (292, 45), (187, 45)], close=True, dxfattribs={"layer": "TITLE"})
    for y in (15, 25, 35):
        line((187, y), (292, y), "TITLE")
    line((262, 5), (262, 15), "TITLE")
    text("STEEL MOUNTING BRACKET", 190, 38.5, 3.5, "TITLE")
    text("MATERIAL: S235JR (EN 10025-2), 10 mm PLATE", 190, 29, 2.5, "TITLE")
    text("DRAWING No. LT-0042    SCALE 1:1    mm", 190, 19, 2.5, "TITLE")
    text("LIGHTABLE EXAMPLE", 190, 9, 2.5, "TITLE")
    e["rev"] = text("REV B", 266, 8.5, 3.5, "TITLE")
    # Revision table above the title block: header and row B.
    msp.add_lwpolyline([(187, 45), (292, 45), (292, 59), (187, 59)], close=True, dxfattribs={"layer": "TITLE"})
    line((187, 52), (292, 52), "TITLE")
    line((197, 45), (197, 59), "TITLE")
    text("REV", 189, 54.5, 2.5, "TITLE")
    text("DESCRIPTION", 200, 54.5, 2.5, "TITLE")
    text("B", 190, 47.5, 2.5, "TITLE")
    text("FIRST ISSUE", 200, 47.5, 2.5, "TITLE")

    # The part, clear of the title block and revision table.
    msp.add_lwpolyline([(25, 80), (185, 80), (185, 180), (25, 180)], close=True, dxfattribs={"layer": "OUTLINE"})
    e["holes"] = [msp.add_circle(c, 6.75, dxfattribs={"layer": "HOLES"})
                  for c in [(45, 100), (165, 100), (165, 160), (45, 160)]]
    e["callout"] = text("4x Ø13.5 THRU", 57, 88, 3, "NOTES")
    e["bolts"] = text("BOLTS: 4x M12 ISO 4017 - 8.8", 25, 34, 3, "NOTES")
    text("GENERAL TOLERANCES ISO 2768-m", 25, 27, 2.5, "NOTES")
    text("BREAK ALL SHARP EDGES", 25, 21, 2.5, "NOTES")

    # Dimensions: readable at 1:1 (3.5 mm text, 3 mm arrows), lengths as
    # drawn (the EZDXF style otherwise scales them by 100).
    style = {"dimtxt": 3.5, "dimasz": 3, "dimexe": 2, "dimexo": 2, "dimgap": 1, "dimlfac": 1}

    def dim(base, p1, p2, angle=0):
        d = msp.add_linear_dim(base=base, p1=p1, p2=p2, angle=angle, dimstyle="EZDXF", override=style,
                               dxfattribs={"layer": "DIMS"})
        d.render()
        return d.dimension

    dim((25, 66), (25, 80), (185, 80))                 # overall width 160
    dim((11, 80), (25, 80), (25, 180), angle=90)       # overall height 100
    e["dims"] = {
        "top_edge": dim((25, 192), (25, 180), (45, 160)),             # 20
        "top_pitch": dim((25, 192), (45, 160), (165, 160)),           # 120
        "right_edge": dim((197, 80), (185, 180), (165, 160), angle=90),  # 20
        "right_pitch": dim((197, 80), (165, 160), (165, 100), angle=90),  # 60
    }
    return doc, e


def bracket_rev_c(doc, e):
    """Rev C (ECN-017): bolts M12 -> M16. Holes Ø17.5 (ISO 273 medium for
    M16), moved 5 mm inward for 25 mm edge distance (EN 1993-1-8: at least
    1.2 d0 = 21 mm); callout, bolt note and dimensions follow. The drafter
    clouds only the bolt note. See bracket-rev-c-inventory.md."""
    msp = doc.modelspace()
    for hole, c in zip(e["holes"], [(50, 105), (160, 105), (160, 155), (50, 155)]):
        hole.dxf.center = c
        hole.dxf.radius = 8.75
    e["callout"].dxf.text = "4x Ø17.5 THRU"
    e["bolts"].dxf.text = "BOLTS: 4x M16 ISO 4017 - 8.8"
    for key, p1, p2 in [("top_edge", (25, 180), (50, 155)), ("top_pitch", (50, 155), (160, 155)),
                        ("right_edge", (185, 180), (160, 155)), ("right_pitch", (160, 155), (160, 105))]:
        d = e["dims"][key]
        d.dxf.defpoint2 = p1
        d.dxf.defpoint3 = p2
        d.override().render()

    # Revision markup.
    e["rev"].dxf.text = "REV C"
    title = {"layer": "TITLE"}
    msp.add_lwpolyline([(187, 59), (187, 66), (292, 66), (292, 59)], dxfattribs=title)
    msp.add_line((197, 59), (197, 66), dxfattribs=title)
    for s, x in [("C", 190), ("BOLTS M12 TO M16 (ECN-017)", 200)]:
        t = msp.add_text(s, height=2.5, dxfattribs=title)
        t.set_placement((x, 61.5), align=TextEntityAlignment.LEFT)
    # Revision cloud (a closed polyline of arcs) around the bolt note only.
    x0, y0, x1, y1 = 21, 30, 85, 41
    step = 5.5
    pts = []
    for (ax, ay), (bx, by) in [((x0, y0), (x1, y0)), ((x1, y0), (x1, y1)), ((x1, y1), (x0, y1)),
                               ((x0, y1), (x0, y0))]:
        n = max(1, round(((bx - ax) ** 2 + (by - ay) ** 2) ** 0.5 / step))
        for k in range(n):
            pts.append((ax + (bx - ax) * k / n, ay + (by - ay) * k / n, -0.55))
    msp.add_lwpolyline(pts, format="xyb", close=True, dxfattribs={"layer": "REVISION"})
    # Revision triangle "C" next to the cloud.
    msp.add_lwpolyline([(89, 31), (99, 31), (94, 40)], close=True, dxfattribs={"layer": "REVISION"})
    t = msp.add_text("C", height=3, dxfattribs={"layer": "REVISION"})
    t.set_placement((94, 34.5), align=TextEntityAlignment.MIDDLE_CENTER)


def main():
    doc, e = bracket()
    doc.saveas(OUT / "bracket-rev-b.dxf")
    bracket_rev_c(doc, e)
    doc.saveas(OUT / "bracket-rev-c.dxf")
    doc, e = build()
    doc.saveas(OUT / "plan-rev-a.dxf")
    revise(doc, e)
    doc.saveas(OUT / "plan-rev-b.dxf")


if __name__ == "__main__":
    main()
