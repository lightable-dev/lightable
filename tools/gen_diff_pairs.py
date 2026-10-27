"""Generates the drawing pairs for the visual-diff tests.

Usage: tools/.venv/bin/python tools/gen_diff_pairs.py tests/diff

Each pair is a folder with a.dxf (before), b.dxf (after) and expected.json:
{"changes": [{"kind": "added|deleted|changed", "type": "LINE", "handle": "...",
"details": ["words that must appear in the change's details"]}],
"zero_geometry_changes": bool}. Handles are a's for deleted/changed and b's
for added. Edits are made with ezdxf on a copy of the base drawing, so
unchanged entities keep their handles; the "renumbered" pair rebuilds the
drawing from scratch in another order instead.

The FreeCAD pair (freecad_resave) is made separately by
tools/freecad_resave.py, since FreeCAD is not available everywhere.
"""

import json
import os
import math
import random
import sys
from pathlib import Path

import ezdxf
from ezdxf.enums import TextEntityAlignment

OUT = Path(sys.argv[1])


def base(doc, order_seed=None):
    """The base drawing; returns {name: entity}. With `order_seed`, the
    same content is created in a shuffled order (new handles, new order)."""
    for name, color in [("WALLS", 7), ("DOORS", 3), ("TEXT", 2), ("HATCH", 4), ("OTHER", 1)]:
        if name not in doc.layers:
            doc.layers.add(name, color=color)
    if "BOLT" not in doc.blocks:
        bolt = doc.blocks.new("BOLT")
        bolt.add_circle((0, 0), 2)
        bolt.add_line((-3, 0), (3, 0))
        bolt.add_attdef("TAG", (0, 3), dxfattribs={"height": 1})
    msp = doc.modelspace()
    makers = [
        ("wall1", lambda: msp.add_line((0, 0), (100, 0), dxfattribs={"layer": "WALLS"})),
        ("wall2", lambda: msp.add_line((100, 0), (100, 60), dxfattribs={"layer": "WALLS"})),
        ("wall3", lambda: msp.add_line((100, 60), (0, 60), dxfattribs={"layer": "WALLS"})),
        ("wall4", lambda: msp.add_line((0, 60), (0, 0), dxfattribs={"layer": "WALLS"})),
        ("door", lambda: msp.add_arc((20, 0), 9, 0, 90, dxfattribs={"layer": "DOORS"})),
        ("column", lambda: msp.add_circle((50, 30), 4, dxfattribs={"layer": "WALLS"})),
        ("outline", lambda: msp.add_lwpolyline([(10, 10, 0), (30, 10, 0.5), (30, 20, 0), (10, 20, 0)],
                                               format="xyb", close=True, dxfattribs={"layer": "OTHER"})),
        ("label", lambda: msp.add_text("LIVING", height=3, dxfattribs={"layer": "TEXT"}).set_placement((40, 45))),
        ("note", lambda: msp.add_mtext("Note one\\PNote two", dxfattribs={"layer": "TEXT", "char_height": 2}).set_location((60, 20))),
        ("bolt", lambda: msp.add_blockref("BOLT", (80, 10), dxfattribs={"layer": "OTHER"})),
        ("fill", lambda: hatch(msp, [(70, 40), (90, 40), (90, 55), (70, 55)], "ANSI31")),
        ("point", lambda: msp.add_point((5, 55), dxfattribs={"layer": "OTHER"})),
    ]
    if order_seed is not None:
        random.Random(order_seed).shuffle(makers)
    out = {}
    for name, make in makers:
        out[name] = make()
    if "bolt" in out:
        out["bolt"].add_auto_attribs({"TAG": "B1"})
    return out


def hatch(msp, points, pattern):
    h = msp.add_hatch(color=256, dxfattribs={"layer": "HATCH"})
    h.paths.add_polyline_path(points, is_closed=True)
    h.set_pattern_fill(pattern, scale=0.5)
    return h


def fresh(version="R2018"):
    return ezdxf.new(version, setup=False)


def save(folder, a, b, changes, zero=False):
    d = OUT / folder
    d.mkdir(parents=True, exist_ok=True)
    a.saveas(d / "a.dxf")
    b.saveas(d / "b.dxf")
    (d / "expected.json").write_text(json.dumps({"changes": changes, "zero_geometry_changes": zero}, indent=2) + "\n")


def pair(folder, edit, version="R2018"):
    """Builds the base, saves it as a, applies `edit` (which returns the
    expected changes) to the same document and saves it as b."""
    doc = fresh(version)
    ents = base(doc)
    d = OUT / folder
    d.mkdir(parents=True, exist_ok=True)
    doc.saveas(d / "a.dxf")
    changes = edit(doc, ents)
    doc.saveas(d / "b.dxf")
    (d / "expected.json").write_text(json.dumps({"changes": changes, "zero_geometry_changes": not changes}, indent=2) + "\n")


def h(e):
    return e.dxf.handle


def moves(doc, e):
    e["column"].translate(10, 5, 0)
    e["label"].translate(-4, 2, 0)
    e["outline"].translate(0, 3, 0)
    return [
        {"kind": "changed", "type": "CIRCLE", "handle": h(e["column"]), "details": ["moved by (10, 5)"]},
        {"kind": "changed", "type": "TEXT", "handle": h(e["label"]), "details": ["moved by (-4, 2)"]},
        {"kind": "changed", "type": "LWPOLYLINE", "handle": h(e["outline"]), "details": ["moved by (0, 3)"]},
    ]


def deletes(doc, e):
    out = [
        {"kind": "deleted", "type": "LINE", "handle": h(e["wall3"]), "details": []},
        {"kind": "deleted", "type": "ARC", "handle": h(e["door"]), "details": []},
        {"kind": "deleted", "type": "POINT", "handle": h(e["point"]), "details": []},
    ]
    msp = doc.modelspace()
    for name in ("wall3", "door", "point"):
        msp.delete_entity(e[name])
    return out


def additions(doc, e):
    msp = doc.modelspace()
    a = msp.add_line((0, 30), (100, 30), dxfattribs={"layer": "WALLS"})
    b = msp.add_circle((20, 40), 3, dxfattribs={"layer": "OTHER"})
    c = msp.add_text("KITCHEN", height=3, dxfattribs={"layer": "TEXT"})
    c.set_placement((10, 45))
    return [
        {"kind": "added", "type": "LINE", "handle": h(a), "details": []},
        {"kind": "added", "type": "CIRCLE", "handle": h(b), "details": []},
        {"kind": "added", "type": "TEXT", "handle": h(c), "details": []},
    ]


def text_and_layers(doc, e):
    e["label"].dxf.text = "LOUNGE"
    e["wall1"].dxf.layer = "OTHER"
    e["column"].dxf.color = 5
    e["note"].text = "Note one\\PNote three"
    e["bolt"].get_attrib("TAG").dxf.text = "B2"
    return [
        {"kind": "changed", "type": "TEXT", "handle": h(e["label"]), "details": ['text changed from "LIVING" to "LOUNGE"']},
        {"kind": "changed", "type": "LINE", "handle": h(e["wall1"]), "details": ["layer changed from WALLS to OTHER"]},
        {"kind": "changed", "type": "CIRCLE", "handle": h(e["column"]), "details": ["colour changed"]},
        {"kind": "changed", "type": "MTEXT", "handle": h(e["note"]), "details": ["text changed"]},
        {"kind": "changed", "type": "INSERT", "handle": h(e["bolt"]), "details": ['attribute TAG changed from "B1" to "B2"']},
    ]


def shapes(doc, e):
    e["column"].dxf.radius = 6
    e["door"].dxf.end_angle = 120
    e["bolt"].dxf.rotation = 45
    e["wall2"].dxf.end = (100, 70)
    return [
        {"kind": "changed", "type": "CIRCLE", "handle": h(e["column"]), "details": ["radius changed from 4 to 6"]},
        {"kind": "changed", "type": "ARC", "handle": h(e["door"]), "details": ["end angle changed from 90 to 120"]},
        {"kind": "changed", "type": "INSERT", "handle": h(e["bolt"]), "details": ["rotation changed from 0 to 45"]},
        {"kind": "changed", "type": "LINE", "handle": h(e["wall2"]), "details": ["end moved by (0, 10)"]},
    ]


def hatches(doc, e):
    e["fill"].set_pattern_fill("ANSI32", scale=0.5)
    msp = doc.modelspace()
    new = hatch(msp, [(5, 5), (8, 5), (8, 8), (5, 8)], "SOLID")
    return [
        {"kind": "changed", "type": "HATCH", "handle": h(e["fill"]), "details": ["pattern changed from ANSI31 to ANSI32", "patterns are not drawn yet"]},
        {"kind": "added", "type": "HATCH", "handle": h(new), "details": []},
    ]


def hatch_boundary(doc, e):
    msp = doc.modelspace()
    old = h(e["fill"])
    msp.delete_entity(e["fill"])
    # Same handle is gone; a new hatch with a different boundary replaces it.
    new = hatch(msp, [(70, 40), (95, 40), (95, 55), (70, 55)], "ANSI31")
    return [
        {"kind": "deleted", "type": "HATCH", "handle": old, "details": []},
        {"kind": "added", "type": "HATCH", "handle": h(new), "details": []},
    ]


def block_edit(doc, e):
    # The block definition changes; its reference does not.
    circle = next(x for x in doc.blocks["BOLT"] if x.dxftype() == "CIRCLE")
    circle.dxf.radius = 3
    return [
        {"kind": "changed", "type": "CIRCLE", "handle": h(circle), "details": ["radius changed from 2 to 3"],
         "used_by": [h(e["bolt"])]},
    ]


def combined(doc, e):
    out = moves(doc, e)
    msp = doc.modelspace()
    out.append({"kind": "deleted", "type": "LINE", "handle": h(e["wall4"]), "details": []})
    msp.delete_entity(e["wall4"])
    a = msp.add_arc((50, 60), 5, 180, 360, dxfattribs={"layer": "DOORS"})
    out.append({"kind": "added", "type": "ARC", "handle": h(a), "details": []})
    e["wall1"].dxf.layer = "DOORS"
    out.append({"kind": "changed", "type": "LINE", "handle": h(e["wall1"]), "details": ["layer changed from WALLS to DOORS"]})
    return out


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    # ONLY=name regenerates one pair (handles in the others stay as they are).
    only = os.environ.get("ONLY")
    if only:
        pair(only, globals()[only])
        return
    pair("moved", moves)
    pair("deleted", deletes)
    pair("added", additions)
    pair("text_and_layers", text_and_layers)
    pair("shapes", shapes)
    pair("hatches", hatches)
    pair("hatch_boundary", hatch_boundary)
    pair("combined", combined)
    pair("block_edit", block_edit)

    # Same content, rebuilt in another order: new handles, new order, new
    # $HANDSEED and timestamps.
    a = fresh()
    base(a)
    b = fresh()
    base(b, order_seed=7)
    save("renumbered", a, b, [], zero=True)

    # Same content, other DXF version from the same writer.
    a = fresh("R2018")
    base(a)
    b = fresh("R2000")
    base(b)
    save("other_version", a, b, [], zero=True)

    # Geometry only, for the FreeCAD re-save (FreeCAD drops text and hatches,
    # and renames every entity's layer after its own object names).
    g = fresh()
    msp = g.modelspace()
    g.layers.add("WALLS", color=7)
    for a_, b_ in [((0, 0), (100, 0)), ((100, 0), (100, 60)), ((100, 60), (0, 60)), ((0, 60), (0, 0))]:
        msp.add_line(a_, b_, dxfattribs={"layer": "WALLS"})
    msp.add_arc((20, 0), 9, 0, 90)
    msp.add_circle((50, 30), 4)
    msp.add_lwpolyline([(10, 10, 0), (30, 10, 0.5), (30, 20, 0), (10, 20, 0)], format="xyb", close=True)
    msp.add_ellipse((60, 50), major_axis=(8, 0), ratio=0.5)
    # Millimetres: with ezdxf's default (metres) FreeCAD scales everything
    # by 1000. No block references: FreeCAD 1.1 explodes them without
    # moving the parts into place (they all land at the block origin).
    g.header["$INSUNITS"] = 4
    d = OUT / "freecad_resave"
    d.mkdir(parents=True, exist_ok=True)
    g.saveas(d / "a.dxf")
    (d / "expected.json").write_text(json.dumps({"changes": [], "zero_geometry_changes": True}, indent=2) + "\n")


if __name__ == "__main__":
    main()
