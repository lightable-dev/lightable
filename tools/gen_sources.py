"""Generates the source drawings for the real-writer DXF corpus with ezdxf.

Usage: tools/.venv/bin/python tools/gen_sources.py OUT_DIR [--big PATH]

Writes each drawing as <name>__ezdxf__<version>.dxf for R2018, R2000 and,
where ezdxf supports every entity used, R12. The R2018 files are the inputs
for tools/convert_corpus.sh. With --big, also writes one ~50 MB mixed file
for the load-speed test.

Everything is deterministic (fixed seeds), apart from the timestamps and
GUIDs ezdxf puts in the header.
"""

import math
import random
import sys
from pathlib import Path

import ezdxf
import ezdxf.math
import ezdxf.render
from ezdxf import colors
from ezdxf.enums import TextEntityAlignment
from ezdxf.lldxf.const import DXFVersionError

VERSIONS = ["R2018", "R2000", "R12"]


def layers(doc, specs):
    for name, color, linetype in specs:
        if name not in doc.layers:
            doc.layers.add(name, color=color, linetype=linetype)


def floor_plan(doc):
    layers(doc, [("WALLS", 7, "Continuous"), ("DOORS", 3, "Continuous"),
                 ("WINDOWS", 4, "Continuous"), ("ROOMS", 2, "Continuous"),
                 ("DIMS", 1, "Continuous"), ("GRID", 8, "DASHED")])
    msp = doc.modelspace()
    outer = [(0, 0), (12000, 0), (12000, 8000), (0, 8000)]
    inner = [(200, 200), (11800, 200), (11800, 7800), (200, 7800)]
    for pts in (outer, inner):
        for a, b in zip(pts, pts[1:] + pts[:1]):
            msp.add_line(a, b, dxfattribs={"layer": "WALLS"})
    for x in (4000, 8000):
        msp.add_line((x, 200), (x, 7800), dxfattribs={"layer": "WALLS"})
    msp.add_line((200, 4000), (4000, 4000), dxfattribs={"layer": "WALLS"})
    for x, y, rot in [(1000, 4000, 0), (5000, 200, 0), (8000, 5000, 90)]:
        msp.add_arc((x, y), 900, rot, rot + 90, dxfattribs={"layer": "DOORS"})
        end = (x + 900 * math.cos(math.radians(rot + 90)),
               y + 900 * math.sin(math.radians(rot + 90)))
        msp.add_line((x, y), end, dxfattribs={"layer": "DOORS"})
    for x in (1500, 6000, 9500):
        msp.add_line((x, 0), (x + 1200, 0), dxfattribs={"layer": "WINDOWS"})
        msp.add_line((x, 200), (x + 1200, 200), dxfattribs={"layer": "WINDOWS"})
    for gx in range(0, 12001, 4000):
        msp.add_line((gx, -1000), (gx, 9000), dxfattribs={"layer": "GRID"})
        msp.add_circle((gx, 9300), 300, dxfattribs={"layer": "GRID"})
    for text, (x, y) in [("LIVING", (2000, 6000)), ("KITCHEN", (2000, 2000)),
                         ("BED 1", (6000, 4000)), ("BATH", (10000, 4000))]:
        msp.add_text(text, height=250, dxfattribs={"layer": "ROOMS"}).set_placement(
            (x, y), align=TextEntityAlignment.MIDDLE_CENTER)
    msp.add_linear_dim(base=(0, -600), p1=(0, 0), p2=(12000, 0),
                       dxfattribs={"layer": "DIMS"}).render()
    msp.add_linear_dim(base=(-600, 0), p1=(0, 0), p2=(0, 8000), angle=90,
                       dxfattribs={"layer": "DIMS"}).render()


def mech_part(doc):
    layers(doc, [("OUTLINE", 7, "Continuous"), ("CENTER", 1, "CENTER"),
                 ("HIDDEN", 5, "HIDDEN"), ("DIMS", 3, "Continuous")])
    msp = doc.modelspace()
    o = {"layer": "OUTLINE"}
    msp.add_line((0, 0), (80, 0), dxfattribs=o)
    msp.add_arc((80, 10), 10, 270, 0, dxfattribs=o)
    msp.add_line((90, 10), (90, 40), dxfattribs=o)
    msp.add_arc((80, 40), 10, 0, 90, dxfattribs=o)
    msp.add_line((80, 50), (0, 50), dxfattribs=o)
    msp.add_line((0, 50), (0, 0), dxfattribs=o)
    for cx in (20, 60):
        msp.add_circle((cx, 25), 6, dxfattribs=o)
        msp.add_circle((cx, 25), 9, dxfattribs={"layer": "HIDDEN"})
        msp.add_line((cx - 12, 25), (cx + 12, 25), dxfattribs={"layer": "CENTER"})
        msp.add_line((cx, 13), (cx, 37), dxfattribs={"layer": "CENTER"})
    d = {"layer": "DIMS"}
    msp.add_linear_dim(base=(0, -10), p1=(0, 0), p2=(90, 0), dxfattribs=d).render()
    msp.add_linear_dim(base=(-10, 0), p1=(0, 0), p2=(0, 50), angle=90, dxfattribs=d).render()
    msp.add_linear_dim(base=(0, 60), p1=(20, 25), p2=(60, 25), dxfattribs=d).render()
    msp.add_aligned_dim(p1=(80, 50), p2=(90, 40), distance=5, dxfattribs=d).render()
    msp.add_radius_dim(center=(80, 10), radius=10, angle=315, dxfattribs=d).render()
    msp.add_diameter_dim(center=(20, 25), radius=6, angle=135, dxfattribs=d).render()
    msp.add_angular_dim_cra(center=(80, 40), radius=10, start_angle=0, end_angle=90,
                            distance=8, dxfattribs=d).render()


def text_styles(doc):
    for name, font in [("SANS", "DejaVuSans.ttf"), ("SERIF", "DejaVuSerif.ttf"),
                       ("MONO", "DejaVuSansMono.ttf"), ("SIMPLEX", "simplex.shx")]:
        if name not in doc.styles:
            doc.styles.add(name, font=font)
    msp = doc.modelspace()
    y = 0
    for style in ("Standard", "SANS", "SERIF", "MONO", "SIMPLEX"):
        for i, align in enumerate([TextEntityAlignment.LEFT, TextEntityAlignment.CENTER,
                                   TextEntityAlignment.RIGHT, TextEntityAlignment.MIDDLE,
                                   TextEntityAlignment.TOP_LEFT]):
            msp.add_text(f"{style} {align.name}", height=2.5 + i * 0.5,
                         dxfattribs={"style": style, "rotation": i * 15}).set_placement(
                (i * 60, y), align=align)
        y -= 20
    msp.add_text("Wände Ø25 ±0.1 °C µm 配管", height=3,
                 dxfattribs={"style": "SANS"}).set_placement((0, y))
    msp.add_text("Oblique and wide", height=3,
                 dxfattribs={"oblique": 15, "width": 1.5}).set_placement((0, y - 10))
    if doc.dxfversion > "AC1009":
        mt = msp.add_mtext("Line one\\PLine two \\fArial|b1|i0;bold\\f;\\C1;red\\C256;"
                           "\\PStacked \\S1/2; and \\H2x;big\\H0.5x;", dxfattribs={
                               "style": "SANS", "char_height": 2.5, "width": 80})
        mt.set_location((0, y - 30))
        msp.add_mtext("Ünïcödé MTEXT ✓ 日本語", dxfattribs={"char_height": 3}).set_location(
            (100, y - 30))


def nested_blocks(doc):
    bolt = doc.blocks.new("BOLT")
    bolt.add_circle((0, 0), 2)
    bolt.add_line((-3, 0), (3, 0))
    bolt.add_line((0, -3), (0, 3))
    plate = doc.blocks.new("PLATE", base_point=(0, 0))
    plate.add_line((0, 0), (40, 0))
    plate.add_line((40, 0), (40, 20))
    plate.add_line((40, 20), (0, 20))
    plate.add_line((0, 20), (0, 0))
    for x, y in [(5, 5), (35, 5), (35, 15), (5, 15)]:
        plate.add_blockref("BOLT", (x, y))
    plate.add_attdef("PARTNO", (2, 22), dxfattribs={"height": 2})
    plate.add_attdef("MAT", (2, -4), dxfattribs={"height": 1.5})
    frame = doc.blocks.new("FRAME")
    frame.add_blockref("PLATE", (0, 0))
    frame.add_blockref("PLATE", (50, 0), dxfattribs={"rotation": 90})
    frame.add_blockref("PLATE", (0, 50), dxfattribs={"xscale": 0.5, "yscale": 0.5})
    msp = doc.modelspace()
    for i in range(3):
        ref = msp.add_blockref("PLATE", (i * 60, 0), dxfattribs={"rotation": i * 10})
        ref.add_auto_attribs({"PARTNO": f"P-{100 + i}", "MAT": "S235"})
    msp.add_blockref("FRAME", (0, 100))
    msp.add_blockref("FRAME", (200, 100), dxfattribs={"xscale": -1})
    msp.add_blockref("BOLT", (0, -50), dxfattribs={
        "column_count": 5, "row_count": 3, "column_spacing": 10, "row_spacing": 10})


def polylines(doc):
    msp = doc.modelspace()
    msp.add_polyline2d([(0, 0), (10, 0), (10, 10)], dxfattribs={"layer": "P2D"})
    msp.add_polyline2d([(20, 0), (30, 0), (30, 10), (20, 10)], close=True)
    msp.add_polyline3d([(0, 20, 0), (10, 20, 5), (20, 30, 10)])
    if doc.dxfversion > "AC1009":
        msp.add_lwpolyline([(40, 0, 0), (50, 0, 1), (50, 10, 0), (40, 10, -0.5)],
                           format="xyb", close=True)
        msp.add_lwpolyline([(60, 0, 0.5, 2, 0), (70, 0, 1, 1, 0), (80, 0, 0.1, 0.1, 0)],
                           format="xyseb")
        msp.add_lwpolyline([(i, math.sin(i / 3) * 5 + 30) for i in range(0, 60)])
        msp.add_lwpolyline([(0, 50), (10, 50)], dxfattribs={"const_width": 1.5,
                                                            "elevation": 3})


def ellipses_splines(doc):
    msp = doc.modelspace()
    msp.add_ellipse((0, 0), major_axis=(10, 0), ratio=0.5)
    msp.add_ellipse((30, 0), major_axis=(0, 8), ratio=0.3,
                    start_param=0, end_param=math.pi)
    msp.add_ellipse((60, 0), major_axis=(7, 7), ratio=0.6,
                    start_param=math.pi / 4, end_param=7 * math.pi / 4)
    msp.add_spline([(0, 20), (10, 30), (20, 20), (30, 30), (40, 20)])
    msp.add_open_spline([(0, 40), (10, 50), (20, 40), (30, 50)], degree=3)
    msp.add_rational_spline([(40, 40), (50, 50), (60, 40), (70, 50)],
                            weights=[1, 2, 2, 1], degree=3)
    msp.add_spline([(80, 0), (90, 10), (100, 0), (90, -10), (80, 0)]).closed = True


def site_mixed(doc):
    rnd = random.Random(7)
    layers(doc, [("TREES", 3, "Continuous"), ("SURVEY", 1, "Continuous"),
                 ("BOUNDARY", 6, "DASHED")])
    msp = doc.modelspace()
    for _ in range(120):
        x, y = rnd.uniform(0, 500), rnd.uniform(0, 300)
        msp.add_circle((x, y), rnd.uniform(1, 4), dxfattribs={"layer": "TREES"})
        msp.add_point((x, y), dxfattribs={"layer": "SURVEY"})
    pts = [(0, 0), (500, 0), (520, 300), (-10, 310)]
    for a, b in zip(pts, pts[1:] + pts[:1]):
        msp.add_line(a, b, dxfattribs={"layer": "BOUNDARY"})
    for i in range(20):
        msp.add_text(f"{rnd.uniform(10, 20):.2f}", height=2,
                     dxfattribs={"layer": "SURVEY"}).set_placement((i * 25, 320))


def annotation_mixed(doc):
    msp = doc.modelspace()
    msp.add_line((0, 0), (50, 0))
    msp.add_circle((25, 20), 10)
    msp.add_leader([(40, 30), (55, 45), (70, 45)])
    msp.add_text("SEE NOTE 3", height=2.5).set_placement((71, 44))
    msp.add_linear_dim(base=(0, -8), p1=(0, 0), p2=(50, 0)).render()
    msp.add_radius_dim(center=(25, 20), radius=10, angle=45).render()
    if doc.dxfversion > "AC1009":
        msp.add_mtext("NOTES:\\P1. ALL DIMS IN MM\\P2. BREAK SHARP EDGES\\P3. FINISH Ra 1.6",
                      dxfattribs={"char_height": 2.5}).set_location((0, 60))
        ml = msp.add_multileader_mtext("Standard")
        ml.set_content("MLEADER TEXT")
        ml.add_leader_line(ezdxf.render.ConnectionSide.left, [(20, 80)])
        ml.build(insert=ezdxf.math.Vec2(40, 90))


def layers_linetypes(doc):
    msp = doc.modelspace()
    ltypes = [lt for lt in ("Continuous", "DASHED", "CENTER", "HIDDEN", "PHANTOM", "DOT",
                            "DASHDOT", "BORDER", "DIVIDE") if lt in doc.linetypes]
    for i in range(40):
        name = f"L{i:02d}_{ltypes[i % len(ltypes)].upper()}"
        doc.layers.add(name, color=(i % 254) + 1, linetype=ltypes[i % len(ltypes)])
        msp.add_line((0, i * 5), (100, i * 5), dxfattribs={"layer": name})
    lay = doc.layers.get("L00_CONTINUOUS")
    lay.off()
    doc.layers.get("L01_DASHED").freeze()
    doc.layers.get("L02_CENTER").lock()
    for i, lw in enumerate([0, 5, 13, 25, 35, 50, 70, 100, 140, 211, -1, -2, -3]):
        attrs = {"lineweight": lw} if doc.dxfversion > "AC1009" else {}
        msp.add_line((120, i * 5), (200, i * 5), dxfattribs=attrs)
    for i, (r, g, b) in enumerate([(255, 0, 0), (0, 128, 255), (12, 34, 56), (250, 250, 0)]):
        attrs = {"color": 3}
        if doc.dxfversion > "AC1009":
            attrs["true_color"] = colors.rgb2int((r, g, b))
        msp.add_circle((220 + i * 15, 0), 5, dxfattribs=attrs)
    msp.add_line((0, -10), (100, -10), dxfattribs={"color": 0})
    msp.add_line((0, -15), (100, -15), dxfattribs={"linetype": "DASHED", "ltscale": 2})


def xdata_groups(doc):
    doc.appids.new("CADCORPUS")
    msp = doc.modelspace()
    line = msp.add_line((0, 0), (10, 0))
    line.set_xdata("CADCORPUS", [(1000, "tag"), (1040, 3.25), (1070, 12),
                                 (1010, (1, 2, 3)), (1002, "{"), (1000, "nested"),
                                 (1002, "}")])
    circle = msp.add_circle((5, 5), 3)
    circle.set_xdata("CADCORPUS", [(1000, "Ünïcödé xdata")])
    if doc.dxfversion > "AC1009":
        arc = msp.add_arc((20, 0), 5, 30, 300)
        doc.groups.new("PAIR").extend([line, circle])
        arc.set_reactors([line.dxf.handle])
        xdict = arc.new_extension_dict()
        xdict.add_dictionary_var("NOTE", "extension dictionary value")


def paperspace_layout(doc):
    msp = doc.modelspace()
    msp.add_circle((0, 0), 50)
    msp.add_line((-60, 0), (60, 0))
    msp.add_text("MODEL", height=5).set_placement((0, 60))
    if doc.dxfversion > "AC1009":
        layout = doc.layouts.new("SHEET-A3")
        layout.page_setup(size=(420, 297), margins=(10, 10, 10, 10), units="mm")
        layout.add_line((0, 0), (400, 0))
        layout.add_text("TITLE BLOCK", height=5).set_placement((300, 10))
        layout.add_viewport(center=(200, 150), size=(300, 200),
                            view_center_point=(0, 0), view_height=150)
    else:
        doc.layout("PAPERSPACE" if "PAPERSPACE" in doc.layout_names() else
                   doc.layout_names()[-1]).add_text("PAPER TEXT", height=5)


def hatch_solids(doc):
    msp = doc.modelspace()
    msp.add_solid([(0, 0), (10, 0), (0, 10), (10, 10)])
    msp.add_3dface([(20, 0, 0), (30, 0, 0), (30, 10, 5), (20, 10, 5)])
    msp.add_trace([(40, 0), (50, 0), (40, 2), (50, 2)])
    if doc.dxfversion > "AC1009":
        msp.add_xline((0, 0), (1, 1))
        msp.add_ray((0, 0), (1, -1))
        h = msp.add_hatch(color=2)
        h.paths.add_polyline_path([(0, 20), (20, 20), (20, 40), (0, 40)], is_closed=True)
        h.set_pattern_fill("ANSI31", scale=0.5)
        s = msp.add_hatch(color=5)
        s.paths.add_polyline_path([(30, 20), (50, 20), (40, 40)], is_closed=True)
    if doc.dxfversion >= "AC1018":
        g = msp.add_hatch()
        g.paths.add_polyline_path([(60, 20), (80, 20), (80, 40), (60, 40)], is_closed=True)
        g.set_gradient((255, 0, 0), (0, 0, 255), rotation=45)


def everything_mixed(doc):
    for build in (floor_plan, mech_part, nested_blocks, polylines):
        build(doc)
    msp = doc.modelspace()
    if doc.dxfversion > "AC1009":
        msp.add_ellipse((200, 200), major_axis=(20, 5), ratio=0.4)
        msp.add_spline([(200, 250), (220, 270), (240, 250)])
        msp.add_mtext("Mixed drawing", dxfattribs={"char_height": 5}).set_location((200, 300))


DRAWINGS = {
    "floor_plan": floor_plan,
    "mech_part": mech_part,
    "text_styles": text_styles,
    "nested_blocks": nested_blocks,
    "polylines": polylines,
    "ellipses_splines": ellipses_splines,
    "site_mixed": site_mixed,
    "annotation_mixed": annotation_mixed,
    "layers_linetypes": layers_linetypes,
    "xdata_groups": xdata_groups,
    "paperspace_layout": paperspace_layout,
    "hatch_solids": hatch_solids,
    "everything_mixed": everything_mixed,
}


def big_file(path, target_bytes=50_000_000):
    """Writes a mixed-entity R2018 file of roughly target_bytes."""
    rnd = random.Random(42)
    doc = ezdxf.new("R2018", setup=True)
    for i in range(20):
        doc.layers.add(f"BIG{i:02d}", color=i + 1)
    blk = doc.blocks.new("MARK")
    blk.add_circle((0, 0), 1)
    blk.add_line((-1, 0), (1, 0))
    msp = doc.modelspace()
    # About 182 bytes per entity on average in R2018 output (measured).
    count = target_bytes // 182
    for i in range(count):
        a = {"layer": f"BIG{i % 20:02d}"}
        x, y = rnd.uniform(0, 1e5), rnd.uniform(0, 1e5)
        kind = i % 8
        if kind < 3:
            msp.add_line((x, y), (x + rnd.uniform(-50, 50), y + rnd.uniform(-50, 50)), dxfattribs=a)
        elif kind == 3:
            msp.add_circle((x, y), rnd.uniform(0.5, 20), dxfattribs=a)
        elif kind == 4:
            msp.add_arc((x, y), rnd.uniform(0.5, 20), rnd.uniform(0, 360), rnd.uniform(0, 360), dxfattribs=a)
        elif kind == 5:
            msp.add_lwpolyline([(x, y, 0), (x + 5, y, 0.5), (x + 5, y + 5, 0), (x, y + 5, 0)],
                               format="xyb", close=True, dxfattribs=a)
        elif kind == 6:
            msp.add_text(f"T{i}", height=1.0, dxfattribs=a).set_placement((x, y))
        else:
            msp.add_blockref("MARK", (x, y), dxfattribs=a)
    doc.saveas(path)


def main():
    args = sys.argv[1:]
    if not args:
        sys.exit(__doc__)
    out = Path(args[0])
    out.mkdir(parents=True, exist_ok=True)
    for name, build in DRAWINGS.items():
        for version in VERSIONS:
            doc = ezdxf.new(version, setup=True)
            try:
                build(doc)
            except DXFVersionError as err:
                print(f"skip {name} {version}: {err}")
                continue
            doc.saveas(out / f"{name}__ezdxf__{version}.dxf")
    if "--big" in args:
        big_file(Path(args[args.index("--big") + 1]))


if __name__ == "__main__":
    main()
