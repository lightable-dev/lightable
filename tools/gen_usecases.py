"""The demo page's five use-case examples (millimetres, $INSUNITS = 4).

Usage: tools/.venv/bin/python tools/gen_usecases.py crates/lightable/assets

Writes four small pairs into the folder (built into the app as examples).
The large layout (5) is written by crates/lightable/src/depot.rs instead,
so its test needs no generated files; `lightable demo-cues out/demo` writes
the copies tools/demo.mjs drops. Next to each pair, an expected-change
inventory (`<name>-inventory.json` and `.md`) is written here, from the
edits this script makes, before anything is compared; the tests in
crates/lightable-diff/tests/examples.rs check the comparison against it.

1. cover: a gearbox cover (front view with a bolt circle, slots and bore,
   section A-A, notes, title block). ECN-031: bolts M10 -> M12. Holes,
   callout and the hole edges in the section follow; only the bolt note
   is clouded.
2. plate: a precision locating plate. One dowel hole moves 0.5 mm, its
   dimension follows, the position tolerance note goes from ±0.1 to ±0.05.
3. shaft: a stepped shaft, Rev D, and Rev E as a simulated re-export:
   new handles, shuffled order, saved as R2000, one outline
   polyline split into lines and arcs, and exactly one real change (the
   surface finish note).
4. flat: an apartment floor plan with door, window and furniture blocks.
   The door block definition is edited (the leaf drawn with its
   thickness) and one partition wall moves 300 mm.
5. depot: a large warehouse layout (tens of thousands of objects) with
   three changes (crates/lightable/src/depot.rs).

Everything is generated here with ezdxf (MIT): ours to ship under the
project's licence.
"""

import json
import random
import sys
from pathlib import Path

import ezdxf
from ezdxf.enums import TextEntityAlignment

ASSETS = Path(sys.argv[1])
ASSETS.mkdir(parents=True, exist_ok=True)
LEFT = TextEntityAlignment.LEFT
MID = TextEntityAlignment.MIDDLE_CENTER
STYLE = {"dimtxt": 3.5, "dimasz": 3, "dimexe": 2, "dimexo": 2, "dimgap": 1, "dimlfac": 1}


class Sheet:
    """A document with layers, an A3 border and a title block."""

    def __init__(self, title, number, rev, layers, version="R2018", size=(420, 297), handles=None):
        self.doc = ezdxf.new(version, setup=True)
        if handles is not None:
            # Another program's numbering: every new object gets a handle
            # from here on, none equal to the original's.
            self.doc.entitydb.handles.reset(handles)
        self.doc.header["$INSUNITS"] = 4
        for name, color in layers + [("BORDER", 8), ("TITLE", 8), ("REVISION", 1)]:
            if name not in self.doc.layers:
                self.doc.layers.add(name, color=color)
        self.msp = self.doc.modelspace()
        w, h = size
        self.size = size
        self.poly([(0, 0), (w, 0), (w, h), (0, h)], "BORDER", close=True)
        x0 = w - 130
        self.poly([(x0, 5), (w - 5, 5), (w - 5, 45), (x0, 45)], "TITLE", close=True)
        for y in (15, 25, 35):
            self.line((x0, y), (w - 5, y), "TITLE")
        self.line((w - 35, 5), (w - 35, 15), "TITLE")
        self.text(title, x0 + 3, 38.5, 3.5, "TITLE")
        self.text("DRAWING No. " + number + "    SCALE AS SHOWN    mm", x0 + 3, 19, 2.5, "TITLE")
        self.text("LIGHTABLE EXAMPLE", x0 + 3, 9, 2.5, "TITLE")
        self.rev = self.text("REV " + rev, w - 31, 8.5, 3.5, "TITLE")
        self.table_x = x0
        self.poly([(x0, 45), (w - 5, 45), (w - 5, 59), (x0, 59)], "TITLE", close=True)
        self.line((x0, 52), (w - 5, 52), "TITLE")
        self.line((x0 + 10, 45), (x0 + 10, 59), "TITLE")
        self.text("REV", x0 + 2, 54.5, 2.5, "TITLE")
        self.text("DESCRIPTION", x0 + 13, 54.5, 2.5, "TITLE")
        self.text(rev, x0 + 3, 47.5, 2.5, "TITLE")
        self.text("FIRST ISSUE", x0 + 13, 47.5, 2.5, "TITLE")

    def text(self, s, x, y, h, layer, align=LEFT, rotation=0):
        t = self.msp.add_text(s, height=h, rotation=rotation, dxfattribs={"layer": layer})
        t.set_placement((x, y), align=align)
        return t

    def line(self, a, b, layer):
        return self.msp.add_line(a, b, dxfattribs={"layer": layer})

    def poly(self, pts, layer, close=False, fmt="xy"):
        return self.msp.add_lwpolyline(pts, format=fmt, close=close, dxfattribs={"layer": layer})

    def circle(self, c, r, layer):
        return self.msp.add_circle(c, r, dxfattribs={"layer": layer})

    def arc(self, c, r, a0, a1, layer):
        return self.msp.add_arc(c, r, a0, a1, dxfattribs={"layer": layer})

    def dim(self, base, p1, p2, angle=0, style=None):
        d = self.msp.add_linear_dim(base=base, p1=p1, p2=p2, angle=angle, dimstyle="EZDXF",
                                    override=dict(STYLE, **(style or {})), dxfattribs={"layer": "DIMS"})
        d.render()
        return d.dimension

    def diameter(self, c, r, angle, style=None):
        d = self.msp.add_diameter_dim(center=c, radius=r, angle=angle, dimstyle="EZDXF",
                                      override=dict(STYLE, **(style or {})), dxfattribs={"layer": "DIMS"})
        d.render()
        return d.dimension

    def revision(self, rev, description, cloud=None, triangle_at=None):
        """Revision markup: letter, table row, and a cloud with a triangle."""
        self.rev.dxf.text = "REV " + rev
        x0, w = self.table_x, self.size[0]
        self.poly([(x0, 59), (x0, 66), (w - 5, 66), (w - 5, 59)], "TITLE")
        self.line((x0 + 10, 59), (x0 + 10, 66), "TITLE")
        self.text(rev, x0 + 3, 61.5, 2.5, "TITLE")
        self.text(description, x0 + 13, 61.5, 2.5, "TITLE")
        if cloud:
            cx0, cy0, cx1, cy1 = cloud
            pts = []
            for (ax, ay), (bx, by) in [((cx0, cy0), (cx1, cy0)), ((cx1, cy0), (cx1, cy1)),
                                       ((cx1, cy1), (cx0, cy1)), ((cx0, cy1), (cx0, cy0))]:
                n = max(1, round(((bx - ax) ** 2 + (by - ay) ** 2) ** 0.5 / 5.5))
                for k in range(n):
                    pts.append((ax + (bx - ax) * k / n, ay + (by - ay) * k / n, -0.55))
            self.poly(pts, "REVISION", close=True, fmt="xyb")
            tx, ty = triangle_at
            self.poly([(tx, ty), (tx + 10, ty), (tx + 5, ty + 9)], "REVISION", close=True)
            self.text(rev, tx + 5, ty + 3.5, 3, "REVISION", align=MID)


def write_inventory(name, about, design, markup=(), notes=None):
    """The expected changes, as JSON (checked by the tests) and Markdown."""
    data = {"about": about, "design": design, "markup": list(markup)}
    if notes:
        data["notes"] = notes
    (ASSETS / f"{name}-inventory.json").write_text(json.dumps(data, indent=1, ensure_ascii=False) + "\n")
    rows = "\n".join(f"| {d['id']} | {d.get('type', '')} | {d.get('layer', '')} | {'; '.join(d['words'])}"
                     f"{' (clouded)' if d.get('clouded') else ''} |" for d in design)
    mrows = "\n".join(f"| {m['id']} | {m.get('note', '; '.join(m['words']))} |" for m in markup)
    md = f"# {name}: expected-change inventory\n\n{about}\n\nWritten by tools/gen_usecases.py from the edits it makes, " \
         f"before the files are compared.\n\n## Design changes ({len(design)})\n\n| # | Type | Layer | Expected words |\n" \
         f"|---|---|---|---|\n{rows}\n"
    if markup:
        md += f"\n## Revision markup\n\n| # | What |\n|---|---|\n{mrows}\n"
    if notes:
        md += "\n## Not changes\n\n" + "\n".join(f"- {n}" for n in notes) + "\n"
    (ASSETS / f"{name}-inventory.md").write_text(md)


# --- 1. Gearbox cover -------------------------------------------------------

def cover():
    s = Sheet("GEARBOX COVER", "LT-0210", "B",
              [("OUTLINE", 7), ("HOLES", 4), ("CENTER", 6), ("NOTES", 2), ("DIMS", 3), ("SECTION", 5)])
    cx, cy = 130, 160
    s.circle((cx, cy), 120, "OUTLINE")             # outside Ø240
    s.circle((cx, cy), 55, "OUTLINE")              # bore Ø110
    s.circle((cx, cy), 100, "CENTER")              # pitch circle PCD 200
    s.line((cx - 128, cy), (cx + 128, cy), "CENTER")
    s.line((cx, cy - 128), (cx, cy + 128), "CENTER")
    import math
    holes = []
    for k in range(8):
        a = math.radians(k * 45)
        holes.append(s.circle((cx + 100 * math.cos(a), cy + 100 * math.sin(a)), 5.5, "HOLES"))
    # Two kidney slots on R75, 8 wide, from 30° to 60° and 210° to 240°.
    for a0 in (30, 210):
        a1 = a0 + 30
        s.arc((cx, cy), 79, a0, a1, "OUTLINE")
        s.arc((cx, cy), 71, a0, a1, "OUTLINE")
        for a in (a0, a1):
            r = math.radians(a)
            ec = (cx + 75 * math.cos(r), cy + 75 * math.sin(r))
            s.arc(ec, 4, a + (0 if a == a1 else 180), a + (180 if a == a1 else 360), "OUTLINE")
    # Four M8 tapped holes on PCD 130 for the bearing retainer.
    for k in range(4):
        a = math.radians(45 + k * 90)
        p = (cx + 65 * math.cos(a), cy + 65 * math.sin(a))
        s.circle(p, 3.4, "HOLES")
        s.arc(p, 4, 0, 270, "HOLES")
    # Section line A-A (vertical, through the centre) with its arrows.
    for y in (cy + 135, cy - 135):
        s.line((cx, y - 4), (cx, y + 4), "SECTION")
        s.poly([(cx, y), (cx + 8, y), (cx + 6, y + 1.5), (cx + 8, y), (cx + 6, y - 1.5)], "SECTION")
        s.text("A", cx + 10, y - 1.5, 3.5, "SECTION")
    # Section A-A: flange 12 thick, boss 30 deep around the bore, the
    # bolt holes at 90° and 270° cut by the plane.
    x0 = 300
    s.text("SECTION A-A", x0 - 5, cy + 132, 3.5, "NOTES")
    s.poly([(x0, cy + 120), (x0 + 12, cy + 120), (x0 + 12, cy + 70), (x0 + 30, cy + 70), (x0 + 30, cy + 55),
            (x0, cy + 55)], "OUTLINE", close=True)
    s.poly([(x0, cy - 120), (x0 + 12, cy - 120), (x0 + 12, cy - 70), (x0 + 30, cy - 70), (x0 + 30, cy - 55),
            (x0, cy - 55)], "OUTLINE", close=True)
    section_edges = []
    for yc in (cy + 100, cy - 100):
        for dy in (5.5, -5.5):
            section_edges.append((s.line((x0, yc + dy), (x0 + 12, yc + dy), "SECTION"), yc, dy))
    s.line((x0 - 6, cy), (x0 + 36, cy), "CENTER")
    # Dimensions.
    s.diameter((cx, cy), 120, 200)
    s.diameter((cx, cy), 55, 160)
    s.diameter((cx, cy), 100, 115)
    s.dim((x0, cy - 140), (x0, cy - 120), (x0 + 12, cy - 120))
    s.dim((x0, cy - 150), (x0, cy - 120), (x0 + 30, cy - 70))
    # Notes.
    callout = s.text("8x Ø11 THRU, EQ. SP. ON PCD 200", 20, 24, 3, "NOTES")
    bolts = s.text("BOLTS: 8x M10x30 ISO 4762 - 8.8", 20, 17, 3, "NOTES")
    s.text("2x SLOT R75 W8, 30°", 20, 31, 3, "NOTES")
    s.text("4x M8 ON PCD 130", 20, 38, 3, "NOTES")
    s.text("MATERIAL: EN AC-42100 T6", 150, 31, 3, "NOTES")
    s.text("GENERAL TOLERANCES ISO 2768-m", 150, 24, 2.5, "NOTES")
    s.text("FACE A FLAT WITHIN 0.05", 150, 17, 2.5, "NOTES")
    s.doc.saveas(ASSETS / "cover-rev-b.dxf")

    # Rev C, ECN-031: bolts M10 -> M12 (higher clamp load). Clearance
    # holes Ø11 -> Ø13.5 (ISO 273 medium); callout and section follow.
    for h in holes:
        h.dxf.radius = 6.75
    callout.dxf.text = "8x Ø13.5 THRU, EQ. SP. ON PCD 200"
    bolts.dxf.text = "BOLTS: 8x M12x30 ISO 4762 - 8.8"
    for ln, yc, dy in section_edges:
        y = yc + (6.75 if dy > 0 else -6.75)
        ln.dxf.start = (x0, y)
        ln.dxf.end = (x0 + 12, y)
    s.revision("C", "BOLTS M10 TO M12 (ECN-031)", cloud=(16, 13, 104, 22), triangle_at=(108, 13))
    s.doc.saveas(ASSETS / "cover-rev-c.dxf")

    design = [{"id": "E1", "type": "TEXT", "layer": "NOTES", "clouded": True,
               "words": ["Text changed from BOLTS: 8x M10x30 ISO 4762 - 8.8 to BOLTS: 8x M12x30 ISO 4762 - 8.8"]}]
    design += [{"id": f"E{2 + k}", "type": "CIRCLE", "layer": "HOLES",
                "words": ["diameter changed from 11 mm to 13.5 mm"]} for k in range(8)]
    design.append({"id": "E10", "type": "TEXT", "layer": "NOTES",
                   "words": ["Text changed from 8x Ø11 THRU, EQ. SP. ON PCD 200 to 8x Ø13.5 THRU, EQ. SP. ON PCD 200"]})
    for k, (_, yc, dy) in enumerate(section_edges):
        design.append({"id": f"E{11 + k}", "type": "LINE", "layer": "SECTION",
                       "words": ["moved 1.25 mm " + ("up" if dy > 0 else "down")]})
    write_inventory(
        "cover",
        "Gearbox cover LT-0210, Rev B -> Rev C. ECN-031: bolts M10 -> M12 for a higher clamp load. "
        "Consequences: clearance holes Ø11 -> Ø13.5 (ISO 273 medium series), the hole callout, and the hole "
        "edges where section A-A cuts two of the holes. The drafter clouds only the bolt note.",
        design,
        [{"id": "M1", "type": "TEXT", "layer": "TITLE", "words": ["Text changed from REV B to REV C"]},
         {"id": "M2", "layer": "TITLE", "words": ["added"], "note": "revision-table row C"},
         {"id": "M3", "type": "LWPOLYLINE", "layer": "REVISION", "words": ["Revision cloud added"]},
         {"id": "M4", "layer": "REVISION", "words": ["added"], "note": "revision triangle and its letter"}])


# --- 2. Precision locating plate ---------------------------------------------

def plate():
    s = Sheet("LOCATING PLATE", "LT-0115", "B",
              [("OUTLINE", 7), ("HOLES", 4), ("CENTER", 6), ("NOTES", 2), ("DIMS", 3)])
    ox, oy = 60, 110
    w, h = 160, 100
    c = 3
    s.poly([(ox + c, oy), (ox + w - c, oy), (ox + w, oy + c), (ox + w, oy + h - c), (ox + w - c, oy + h),
            (ox + c, oy + h), (ox, oy + h - c), (ox, oy + c)], "OUTLINE", close=True)
    # Two dowel holes Ø8 H7 (datum B and C), four M6 tapped holes, a pocket.
    dowels = [s.circle((ox + 20, oy + 20), 4, "HOLES"), s.circle((ox + 80, oy + 80), 4, "HOLES")]
    for x, y in [(ox + 140, oy + 20), (ox + 140, oy + 80), (ox + 20, oy + 80), (ox + 80, oy + 20)]:
        s.circle((x, y), 2.5, "HOLES")
        s.arc((x, y), 3, 0, 270, "HOLES")
    s.poly([(ox + 100, oy + 35), (ox + 125, oy + 35), (ox + 125, oy + 65), (ox + 100, oy + 65)], "OUTLINE",
           close=True)
    for x, y in [(ox + 20, oy + 20), (ox + 80, oy + 80)]:
        s.line((x - 7, y), (x + 7, y), "CENTER")
        s.line((x, y - 7), (x, y + 7), "CENTER")
    fine = {"dimdec": 2, "dimzin": 8}
    s.dim((ox, oy - 14), (ox, oy), (ox + w, oy))
    s.dim((ox - 14, oy), (ox, oy), (ox, oy + h), angle=90)
    s.dim((ox, oy - 7), (ox, oy), (ox + 20, oy + 20), style=fine)
    moved_dim = s.dim((ox, oy + h + 10), (ox, oy + h), (ox + 80, oy + 80), style=fine)
    s.dim((ox - 7, oy), (ox, oy), (ox + 20, oy + 20), angle=90, style=fine)
    s.dim((ox + w + 10, oy), (ox + w, oy), (ox + 80, oy + 80), angle=90, style=fine)
    s.text("2x Ø8 H7 THRU (DATUM B, C)", 20, 38, 3, "NOTES")
    s.text("4x M6 - 6H THRU", 20, 31, 3, "NOTES")
    tol = s.text("HOLE POSITIONS ±0.1", 20, 24, 3, "NOTES")
    s.text("MATERIAL: 1.2312, 32 HRC", 150, 31, 3, "NOTES")
    s.text("GENERAL TOLERANCES ISO 2768-f", 150, 24, 2.5, "NOTES")
    s.doc.saveas(ASSETS / "plate-rev-b.dxf")

    # Rev C: the second dowel hole 0.5 mm further out (mating part), and
    # tighter hole positions.
    dowels[1].dxf.center = (ox + 80.5, oy + 80)
    moved_dim.dxf.defpoint3 = (ox + 80.5, oy + 80)
    moved_dim.override().render()
    tol.dxf.text = "HOLE POSITIONS ±0.05"
    s.revision("C", "DOWEL B MOVED, TOLERANCE TIGHTENED")
    s.doc.saveas(ASSETS / "plate-rev-c.dxf")
    write_inventory(
        "plate",
        "Locating plate LT-0115, Rev B -> Rev C: the second dowel hole moves 0.5 mm in X to suit the mating "
        "part, its position dimension follows (80 -> 80.5), and the hole-position tolerance tightens from "
        "±0.1 to ±0.05. Nothing is clouded.",
        [{"id": "E1", "type": "CIRCLE", "layer": "HOLES", "words": ["moved 0.5 mm right"]},
         {"id": "E2", "type": "DIMENSION", "layer": "DIMS", "words": ["Dimension changed from 80 mm to 80.5 mm"]},
         {"id": "E3", "type": "TEXT", "layer": "NOTES",
          "words": ["Text changed from HOLE POSITIONS ±0.1 to HOLE POSITIONS ±0.05"]}],
        [{"id": "M1", "type": "TEXT", "layer": "TITLE", "words": ["Text changed from REV B to REV C"]},
         {"id": "M2", "layer": "TITLE", "words": ["added"], "note": "revision-table row C"}])


# --- 3. Re-export noise ------------------------------------------------------

def shaft(version, order_seed, finish, split_outline, handles=None, revised=False):
    """The shaft drawing. Each part is a function; `order_seed` shuffles the
    order they are written in (so handles and entity order differ),
    `split_outline` writes the outline as lines and arcs."""
    s = Sheet("DRIVE SHAFT", "LT-0020", "D",
              [("OUTLINE", 7), ("CENTER", 6), ("HIDDEN", 5), ("NOTES", 2), ("DIMS", 3)], version=version,
              handles=handles)
    y0 = 160
    # Stepped shaft: Ø30 x 40, Ø40 x 120, Ø30 x 60, chamfers 1x45 at the ends.
    profile = [(40, y0 - 14), (41, y0 - 15), (80, y0 - 15), (80, y0 - 20), (200, y0 - 20), (200, y0 - 15),
               (259, y0 - 15), (260, y0 - 14), (260, y0 + 14), (259, y0 + 15), (200, y0 + 15), (200, y0 + 20),
               (80, y0 + 20), (80, y0 + 15), (41, y0 + 15), (40, y0 + 14)]
    parts = []

    def outline():
        if split_outline:
            for a, b in zip(profile, profile[1:] + profile[:1]):
                s.line(a, b, "OUTLINE")
        else:
            s.poly(profile, "OUTLINE", close=True)
    parts.append(outline)
    parts.append(lambda: [s.line((x, y0 - 15 if x != 200 and x != 80 else y0 - 20),
                                 (x, y0 + 15 if x != 200 and x != 80 else y0 + 20), "OUTLINE")
                          for x in (80, 200)])
    parts.append(lambda: s.line((32, y0), (268, y0), "CENTER"))
    # Keyway 8 wide, 40 long, on the Ø40 section (top view lines).
    parts.append(lambda: s.poly([(110, y0 + 4), (146, y0 + 4)], "OUTLINE"))
    parts.append(lambda: s.poly([(110, y0 - 4), (146, y0 - 4)], "OUTLINE"))
    parts.append(lambda: s.arc((110, y0), 4, 90, 270, "OUTLINE"))
    parts.append(lambda: s.arc((146, y0), 4, 270, 90, "OUTLINE"))
    # End view.
    parts.append(lambda: s.circle((330, y0), 20, "OUTLINE"))
    parts.append(lambda: s.circle((330, y0), 15, "HIDDEN"))
    parts.append(lambda: s.line((305, y0), (355, y0), "CENTER"))
    parts.append(lambda: s.line((330, y0 - 25), (330, y0 + 25), "CENTER"))
    parts.append(lambda: s.poly([(326, y0 + 20), (326, y0 + 15.5), (334, y0 + 15.5), (334, y0 + 20)], "OUTLINE"))
    # Notes.
    parts.append(lambda: s.text("CHAMFERS 1x45°", 20, 38, 3, "NOTES"))
    parts.append(lambda: s.text("KEYWAY 8 P9 x 36 (DIN 6885)", 20, 31, 3, "NOTES"))
    parts.append(lambda: s.text(f"SURFACE FINISH {finish}", 20, 24, 3, "NOTES"))
    parts.append(lambda: s.text("MATERIAL: C45E +QT", 150, 31, 3, "NOTES"))
    parts.append(lambda: s.text("GENERAL TOLERANCES ISO 2768-m", 150, 24, 2.5, "NOTES"))
    order = list(range(len(parts)))
    if order_seed is not None:
        random.Random(order_seed).shuffle(order)
    for i in order:
        parts[i]()
    if revised:
        s.revision("E", "SURFACE FINISH Ra 0.8")
    return s


def reexport():
    shaft("R2018", None, "Ra 1.6", False).doc.saveas(ASSETS / "shaft-rev-d.dxf")
    shaft("R2000", 7, "Ra 0.8", True, handles="7000", revised=True).doc.saveas(ASSETS / "shaft-rev-e.dxf")
    write_inventory(
        "shaft",
        "Drive shaft LT-0020, Rev D, and Rev E as a simulated re-export (no CAD program was used): every handle renumbered, the "
        "entities in another order, saved as DXF R2000 instead of R2018, the outline polyline split into "
        "separate lines. One real change: the surface finish note, Ra 1.6 -> Ra 0.8.",
        [{"id": "E1", "type": "TEXT", "layer": "NOTES",
          "words": ["Text changed from SURFACE FINISH Ra 1.6 to SURFACE FINISH Ra 0.8"]}],
        [{"id": "M1", "type": "TEXT", "layer": "TITLE", "words": ["Text changed from REV D to REV E"]},
         {"id": "M2", "layer": "TITLE", "words": ["added"], "note": "revision-table row E"}],
        notes=["handles renumbered, entity order shuffled, file version R2018 -> R2000",
               "the outline polyline (16 vertices) written as 16 lines: drawn the same (redrawn)"])


# --- 4. Floor plan with blocks -------------------------------------------------

def flat():
    doc = ezdxf.new("R2018", setup=True)
    doc.header["$INSUNITS"] = 4
    for name, color in [("WALLS", 7), ("DOORS", 3), ("WINDOWS", 4), ("FURNITURE", 8), ("TEXT", 2),
                        ("DIMS", 1)]:
        doc.layers.add(name, color=color)
    msp = doc.modelspace()
    # Blocks: door 800 (leaf and swing), window 1200, bed, sofa, table with
    # chairs, kitchen counter.
    door = doc.blocks.new("DOOR-800")
    leaf = door.add_line((0, 0), (0, 800))
    door.add_arc((0, 0), 800, 0, 90)
    win = doc.blocks.new("WINDOW-1200")
    for y in (0, 80, 160):
        win.add_line((0, y), (1200, y))
    win.add_line((0, 0), (0, 160))
    win.add_line((1200, 0), (1200, 160))
    bed = doc.blocks.new("BED-1600")
    bed.add_lwpolyline([(0, 0), (1600, 0), (1600, 2000), (0, 2000)], close=True)
    bed.add_lwpolyline([(100, 1650), (750, 1650), (750, 1950), (100, 1950)], close=True)
    bed.add_lwpolyline([(850, 1650), (1500, 1650), (1500, 1950), (850, 1950)], close=True)
    sofa = doc.blocks.new("SOFA-3")
    sofa.add_lwpolyline([(0, 0), (2200, 0), (2200, 900), (0, 900)], close=True)
    sofa.add_lwpolyline([(150, 0), (2050, 0), (2050, 700), (150, 700)], close=True)
    table = doc.blocks.new("TABLE-4")
    table.add_lwpolyline([(0, 0), (1400, 0), (1400, 800), (0, 800)], close=True)
    for x, y in [(250, -450), (750, -450), (250, 850), (750, 850)]:
        table.add_lwpolyline([(x, y), (x + 400, y), (x + 400, y + 400), (x, y + 400)], close=True)
    # Walls: outer 200 thick (two lines each side), partitions 100.
    W, H = 12000, 9000

    def wall(a, b, t):
        (x0, y0), (x1, y1) = a, b
        if y0 == y1:
            return [msp.add_line((x0, y0 - t / 2), (x1, y1 - t / 2), dxfattribs={"layer": "WALLS"}),
                    msp.add_line((x0, y0 + t / 2), (x1, y1 + t / 2), dxfattribs={"layer": "WALLS"})]
        return [msp.add_line((x0 - t / 2, y0), (x1 - t / 2, y1), dxfattribs={"layer": "WALLS"}),
                msp.add_line((x0 + t / 2, y0), (x1 + t / 2, y1), dxfattribs={"layer": "WALLS"})]
    wall((0, 0), (W, 0), 200)
    wall((0, H), (W, H), 200)
    wall((0, 0), (0, H), 200)
    wall((W, 0), (W, H), 200)
    moved = wall((4500, 0), (4500, 5000), 100)        # bedroom 1 | bedroom 2
    wall((0, 5000), (W, 5000), 100)                   # bedrooms | living
    wall((8000, 0), (8000, 5000), 100)                # bedroom 2 | bath
    wall((8000, 2500), (W, 2500), 100)                # bath | storage
    # Doors (insertion at the hinge), windows.
    doors = [((1500, 5050), 270), ((6000, 5050), 270), ((9000, 5050), 270), ((9500, 2550), 0),
             ((10500, 2450), 180), ((200, 7000), 0), ((11800, 7500), 180)]
    for at, rot in doors:
        msp.add_blockref("DOOR-800", at, dxfattribs={"layer": "DOORS", "rotation": rot})
    for at, rot in [((1500, -100), 0), ((5500, -100), 0), ((9200, -100), 0), ((2000, 8900), 0),
                    ((7000, 8900), 0), ((12100, 3500), 90)]:
        msp.add_blockref("WINDOW-1200", at, dxfattribs={"layer": "WINDOWS", "rotation": rot})
    msp.add_blockref("BED-1600", (1400, 1200), dxfattribs={"layer": "FURNITURE"})
    msp.add_blockref("BED-1600", (5400, 1200), dxfattribs={"layer": "FURNITURE"})
    msp.add_blockref("SOFA-3", (1000, 7600), dxfattribs={"layer": "FURNITURE"})
    msp.add_blockref("TABLE-4", (7000, 6800), dxfattribs={"layer": "FURNITURE"})
    msp.add_lwpolyline([(9500, 8900), (11900, 8900), (11900, 8300), (9500, 8300)], close=True,
                       dxfattribs={"layer": "FURNITURE"})
    for s, x, y in [("BEDROOM 1", 2200, 4200), ("BEDROOM 2", 6200, 4200), ("BATH", 9800, 4000),
                    ("STORAGE", 9800, 1200), ("LIVING / KITCHEN", 5600, 7400)]:
        t = msp.add_text(s, height=250, dxfattribs={"layer": "TEXT"})
        t.set_placement((x, y), align=TextEntityAlignment.MIDDLE_CENTER)
    t = msp.add_text("FLAT 3.02 - LIGHTABLE EXAMPLE", height=300, dxfattribs={"layer": "TEXT"})
    t.set_placement((0, -1500), align=TextEntityAlignment.LEFT)
    st = {"dimtxt": 200, "dimasz": 150, "dimexe": 100, "dimexo": 100, "dimgap": 50, "dimlfac": 1}
    msp.add_linear_dim(base=(0, -700), p1=(0, -100), p2=(W, -100), dimstyle="EZDXF", override=st,
                       dxfattribs={"layer": "DIMS"}).render()
    msp.add_linear_dim(base=(-700, 0), p1=(-100, 0), p2=(-100, H), angle=90, dimstyle="EZDXF", override=st,
                       dxfattribs={"layer": "DIMS"}).render()
    doc.saveas(ASSETS / "flat-rev-a.dxf")

    # Rev B: the door leaf drawn with its 40 mm thickness (block edit, shown
    # by all seven doors), and the bedroom partition moved 300 mm right.
    door.delete_entity(leaf)
    door.add_lwpolyline([(0, 0), (40, 0), (40, 800), (0, 800)], close=True)
    for ln in moved:
        ln.dxf.start = (ln.dxf.start.x + 300, ln.dxf.start.y)
        ln.dxf.end = (ln.dxf.end.x + 300, ln.dxf.end.y)
    doc.saveas(ASSETS / "flat-rev-b.dxf")
    write_inventory(
        "flat",
        "Flat 3.02 floor plan, Rev A -> Rev B: the door block DOOR-800 is redrawn with a 40 mm thick leaf (one "
        "edit to the block definition, shown by all 7 doors), and the partition between the two bedrooms "
        "moves 300 mm to the right (its two wall lines).",
        [{"id": "E1", "type": "LINE", "words": ["In block DOOR-800", "deleted"]},
         {"id": "E2", "type": "LWPOLYLINE", "words": ["In block DOOR-800", "added"]},
         {"id": "E3", "type": "LINE", "layer": "WALLS", "words": ["moved 300 mm right"]},
         {"id": "E4", "type": "LINE", "layer": "WALLS", "words": ["moved 300 mm right"]}],
        notes=["the door block is shown by 7 references (INSERT); each shows the edit"])


if __name__ == "__main__":
    cover()
    plate()
    reexport()
    flat()
