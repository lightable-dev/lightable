"""Hatch fixtures for the renderer, with ezdxf's own boundary paths as the
reference.

Usage: tools/.venv/bin/python tools/gen_hatch_fixtures.py tests/hatch

Writes hatches.dxf (one hatch per boundary kind) and reference.json:
{handle: {"solid": bool, "loops": [[[x, y], ...], ...]}}, the boundary
loops flattened by ezdxf.path.from_hatch in world coordinates.
"""

import json
import sys
from pathlib import Path

import ezdxf
from ezdxf import path as ezpath
from ezdxf.math import Vec2

OUT = Path(sys.argv[1])
OUT.mkdir(parents=True, exist_ok=True)

doc = ezdxf.new("R2018", setup=False)
doc.header["$INSUNITS"] = 4
msp = doc.modelspace()
hatches = []


def new(solid=True, color=1, **attribs):
    h = msp.add_hatch(color=color, dxfattribs=attribs)
    if not solid:
        h.set_pattern_fill("ANSI31", scale=0.5)
    hatches.append(h)
    return h


# Polyline path with bulges (a rounded rectangle side).
h = new()
h.paths.add_polyline_path([(0, 0, 0), (10, 0, 0.5), (10, 5, 0), (0, 5, 0)], is_closed=True)

# Edge path: lines and a counter-clockwise arc.
h = new(color=3)
e = h.paths.add_edge_path()
e.add_line((20, 0), (30, 0))
e.add_arc((30, 5), radius=5, start_angle=-90, end_angle=90, ccw=True)
e.add_line((30, 10), (20, 10))
e.add_line((20, 10), (20, 0))

# Edge path with a clockwise arc (angles stored mirrored).
h = new(color=4)
e = h.paths.add_edge_path()
e.add_line((40, 0), (50, 0))
e.add_arc((50, 5), radius=5, start_angle=270, end_angle=90, ccw=False)
e.add_line((50, 10), (40, 10))
e.add_line((40, 10), (40, 0))

# Ellipse edges, both directions.
h = new(color=5)
e = h.paths.add_edge_path()
e.add_ellipse((65, 5), major_axis=(8, 0), ratio=0.5, start_angle=0, end_angle=180, ccw=True)
e.add_line((57, 5), (73, 5))
h = new(color=6)
e = h.paths.add_edge_path()
e.add_ellipse((85, 5), major_axis=(0, 6), ratio=0.5, start_angle=0, end_angle=180, ccw=False)
e.add_line((85, -1), (85, 11))

# Spline edge closed by a line.
h = new(color=2)
e = h.paths.add_edge_path()
e.add_spline(control_points=[(100, 0), (103, 8), (108, 8), (112, 0)], degree=3)
e.add_line((112, 0), (100, 0))

# Pattern hatch with a hole and an island (even-odd).
h = new(solid=False, color=7)
h.paths.add_polyline_path([(0, 20), (30, 20), (30, 40), (0, 40)], is_closed=True)
h.paths.add_polyline_path([(5, 25), (25, 25), (25, 35), (5, 35)], is_closed=True)
h.paths.add_polyline_path([(10, 28), (20, 28), (20, 32), (10, 32)], is_closed=True)

# Mirrored plane (extrusion -Z) with an elevation.
h = new(color=1, extrusion=(0, 0, -1))
h.dxf.elevation = (0, 0, 2)
h.paths.add_polyline_path([(-40, 20), (-50, 20), (-50, 30), (-40, 30)], is_closed=True)

doc.saveas(OUT / "hatches.dxf")

reference = {}
for h in hatches:
    loops = []
    for p in ezpath.from_hatch(h):
        pts = [Vec2(v) for v in p.flattening(distance=0.001)]
        loops.append([[round(v.x, 9), round(v.y, 9)] for v in pts])
    reference[h.dxf.handle] = {"solid": bool(h.dxf.solid_fill), "loops": loops}
(OUT / "reference.json").write_text(json.dumps(reference, separators=(",", ":")) + "\n")
print(f"{len(hatches)} hatches")
