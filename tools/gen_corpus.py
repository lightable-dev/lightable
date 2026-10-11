"""Regenerates the hand-written DXF files in tests/corpus/.

Usage: python3 tools/gen_corpus.py tests/corpus

Each file targets one reader behaviour (versions, line endings, padding,
unknown entities, damaged records, ...). The corpus tests in
crates/lightable-dxf/tests/corpus.rs assert on their exact contents, so update
those tests when changing a file here.
"""
import sys, os
out = sys.argv[1]

def doc(*sections, eof=True):
    p = []
    for s in sections: p += s
    if eof: p.append((0, "EOF"))
    return p

def sec(name, *items):
    p = [(0, "SECTION"), (2, name)]
    for i in items: p += i
    return p + [(0, "ENDSEC")]

def hdr(**vars_):
    p = []
    for k, v in vars_.items():
        p.append((9, "$" + k))
        p += v
    return p

def table(name, header, *entries):
    p = [(0, "TABLE"), (2, name)] + header
    for e in entries: p += e
    return p + [(0, "ENDTAB")]

def layer(name, color=7, lt="CONTINUOUS", flags=0, handle=None, extra=()):
    p = [(0, "LAYER")]
    if handle: p += [(5, handle), (330, "2"), (100, "AcDbSymbolTableRecord"), (100, "AcDbLayerTableRecord")]
    p += [(2, name), (70, str(flags)), (62, str(color)), (6, lt)] + list(extra)
    return p

def line(x1, y1, x2, y2, layer="0", handle=None, sub=False, extra=()):
    p = [(0, "LINE")]
    if handle: p += [(5, handle), (330, "1F")]
    if sub: p += [(100, "AcDbEntity")]
    p += [(8, layer)] + list(extra)
    if sub: p += [(100, "AcDbLine")]
    return p + [(10, x1), (20, y1), (30, "0.0"), (11, x2), (21, y2), (31, "0.0")]

def circle(x, y, r, layer="0", handle=None, sub=False, extra=()):
    p = [(0, "CIRCLE")]
    if handle: p += [(5, handle), (330, "1F")]
    if sub: p += [(100, "AcDbEntity")]
    p += [(8, layer)] + list(extra)
    if sub: p += [(100, "AcDbCircle")]
    return p + [(10, x), (20, y), (30, "0.0"), (40, r)]

def arc(x, y, r, a0, a1, layer="0", handle=None, sub=False, extra=()):
    p = circle(x, y, r, layer, handle, sub, extra)
    p[0] = (0, "ARC")
    if sub: p.append((100, "AcDbArc"))
    return p + [(50, a0), (51, a1)]

def write(name, pairs, nl="\n", pad=False, trailing=True):
    lines = []
    for c, v in pairs:
        lines.append(f"{c:>3}" if pad else str(c))
        lines.append(v)
    text = nl.join(lines) + (nl if trailing else "")
    with open(os.path.join(out, name), "w", encoding="utf-8", newline="") as f:
        f.write(text)

R12 = hdr(ACADVER=[(1, "AC1009")], EXTMIN=[(10, "0.0"), (20, "0.0"), (30, "0.0")], EXTMAX=[(10, "100.0"), (20, "50.0"), (30, "0.0")])

write("r12_single_line.dxf", doc(sec("HEADER", R12), sec("ENTITIES", line("0.0", "0.0", "100.0", "50.0"))))

write("r12_circle_arc_layers.dxf", doc(
    sec("HEADER", R12, hdr(CLAYER=[(8, "GEOMETRY")])),
    sec("TABLES",
        table("LTYPE", [(70, "1")], [(0, "LTYPE"), (2, "CONTINUOUS"), (70, "0"), (3, "Solid line"), (72, "65"), (73, "0"), (40, "0.0")]),
        table("LAYER", [(70, "2")], layer("0"), layer("GEOMETRY", color=3))),
    sec("ENTITIES", circle("50.0", "25.0", "10.0", "GEOMETRY"), arc("50.0", "25.0", "20.0", "0.0", "180.0", "GEOMETRY"))))

AC1015_HDR = hdr(ACADVER=[(1, "AC1015")], DWGCODEPAGE=[(3, "ANSI_1252")], INSUNITS=[(70, "4")], CLAYER=[(8, "0")], TEXTSTYLE=[(7, "Standard")], DIMSTYLE=[(2, "Standard")])
write("ac1015_floor_plan.dxf", doc(
    sec("HEADER", AC1015_HDR),
    sec("TABLES", table("LAYER", [(5, "2"), (330, "0"), (100, "AcDbSymbolTable"), (70, "4")],
        layer("0", handle="10"),
        layer("WALLS", color=1, handle="11", extra=[(370, "50"), (390, "F")]),
        layer("DOORS", color=-4, handle="12"),
        layer("HIDDEN_REF", color=8, lt="DASHED", flags=5, handle="13", extra=[(290, "0")]))),
    sec("ENTITIES",
        line("0.0", "0.0", "6000.0", "0.0", "WALLS", "20", True),
        line("6000.0", "0.0", "6000.0", "4000.0", "WALLS", "21", True),
        line("6000.0", "4000.0", "0.0", "4000.0", "WALLS", "22", True),
        line("0.0", "4000.0", "0.0", "0.0", "WALLS", "23", True),
        arc("900.0", "0.0", "900.0", "0.0", "90.0", "DOORS", "24", True),
        line("900.0", "0.0", "900.0", "900.0", "DOORS", "25", True))))

AC1032_HDR = hdr(ACADVER=[(1, "AC1032")], DWGCODEPAGE=[(3, "ANSI_1252")], INSUNITS=[(70, "6")],
                 EXTMIN=[(10, "-5.0"), (20, "-5.0"), (30, "0.0")], EXTMAX=[(10, "20.0"), (20, "15.0"), (30, "0.0")], CLAYER=[(8, "Site")])
write("ac1032_colors_lineweights.dxf", doc(
    sec("HEADER", AC1032_HDR),
    sec("TABLES", table("LAYER", [(5, "2"), (100, "AcDbSymbolTable"), (70, "2")],
        layer("0", handle="10"), layer("Site", color=5, handle="11", extra=[(420, "16744448"), (370, "-3")]))),
    sec("ENTITIES",
        line("-5.0", "-5.0", "20.0", "15.0", "Site", "30", True, extra=[(62, "1"), (420, "16711680"), (370, "35")]),
        circle("0.0", "0.0", "5.0", "Site", "31", True, extra=[(62, "0"), (6, "BYBLOCK"), (370, "-2")]),
        arc("10.0", "10.0", "2.5", "45.0", "315.0", "Site", "32", True, extra=[(62, "256"), (370, "-1")]),
        circle("3.0", "3.0", "0.0", "0", "33", True))))

crlf = doc(sec("HEADER", hdr(ACADVER=[(1, "AC1015")])),
           sec("ENTITIES", line("1.0", "1.0", "2.0", "2.0"), circle("1.5", "1.5", "0.5"), arc("1.5", "1.5", "1.0", "270.0", "90.0")))
write("crlf_line_endings.dxf", crlf, nl="\r\n")

write("padded_group_codes.dxf", doc(
    sec("HEADER", hdr(ACADVER=[(1, "AC1009")], INSUNITS=[(70, "     1")])),
    sec("ENTITIES",
        [(0, "LINE"), (8, "0"), (10, "  1.25"), (20, "  2.50"), (30, "0.0"), (11, "  3.75"), (21, "  5.00"), (31, "0.0")],
        [(0, "CIRCLE"), (8, "0"), (10, " 1e1"), (20, " -1E-1"), (40, "  .5")])), pad=True)

write("unknown_entities.dxf", doc(
    sec("HEADER", hdr(ACADVER=[(1, "AC1015")])),
    sec("ENTITIES",
        [(0, "LWPOLYLINE"), (5, "40"), (100, "AcDbEntity"), (8, "0"), (100, "AcDbPolyline"), (90, "4"), (70, "1"), (43, "0.0"),
         (10, "0.0"), (20, "0.0"), (10, "10.0"), (20, "0.0"), (42, "0.414213562373095"), (10, "10.0"), (20, "10.0"), (10, "0.0"), (20, "10.0")],
        [(0, "TEXT"), (5, "41"), (8, "ANNOT"), (10, "1.0"), (20, "1.0"), (30, "0.0"), (40, "2.5"), (1, "Room 101"), (50, "0.0")],
        [(0, "POINT"), (8, "0"), (10, "5.0"), (20, "5.0"), (30, "0.0")],
        line("0.0", "0.0", "10.0", "10.0", handle="42", sub=True),
        [(0, "POLYLINE"), (8, "0"), (66, "1"), (10, "0.0"), (20, "0.0"), (30, "0.0"), (70, "0")],
        [(0, "VERTEX"), (8, "0"), (10, "0.0"), (20, "0.0"), (30, "0.0")],
        [(0, "VERTEX"), (8, "0"), (10, "3.0"), (20, "4.0"), (30, "0.0")],
        [(0, "SEQEND"), (8, "0")],
        [(0, "ACME_CUSTOM_THING"), (8, "0"), (1, "private data"), (90, "12"), (310, "DEADBEEF")])))

write("all_sections.dxf", doc(
    [(999, "Hand-written test file for cad-dxf")],
    sec("HEADER", hdr(ACADVER=[(1, "AC1018")], HANDSEED=[(5, "FF")])),
    sec("CLASSES", [(0, "CLASS"), (1, "ACDBDICTIONARYWDFLT"), (2, "AcDbDictionaryWithDefault"), (3, "ObjectDBX Classes"), (90, "0"), (280, "0"), (281, "0")]),
    sec("TABLES", table("VPORT", [(70, "1")], [(0, "VPORT"), (2, "*ACTIVE"), (70, "0"), (10, "0.0"), (20, "0.0")]),
                  table("LAYER", [(70, "1")], layer("0"))),
    sec("BLOCKS", [(0, "BLOCK"), (8, "0"), (2, "*Model_Space"), (70, "0"), (10, "0.0"), (20, "0.0"), (30, "0.0"), (3, "*Model_Space"), (1, "")],
                  [(0, "ENDBLK"), (8, "0")]),
    sec("ENTITIES", circle("0.0", "0.0", "1.0")),
    sec("OBJECTS", [(0, "DICTIONARY"), (5, "C"), (330, "0"), (100, "AcDbDictionary"), (281, "1")]),
    sec("ACDSDATA", [(70, "2"), (71, "6")])))

write("entities_only_no_eof.dxf", doc(sec("ENTITIES", line("0", "0", "1", "0"), line("1", "0", "1", "1")), eof=False))

write("arcs_and_extras.dxf", doc(
    sec("HEADER", hdr(ACADVER=[(1, "AC1027")])),
    sec("ENTITIES",
        arc("0.0", "0.0", "5.0", "350.0", "10.0", handle="50", sub=True),
        arc("0.0", "0.0", "5.0", "0.0", "360.0", handle="51", sub=True),
        arc("-1000000.5", "2000000.25", "0.001", "90.0", "89.999", handle="52", sub=True, extra=[(39, "2.0")]),
        line("0.0", "0.0", "1.0", "0.0", handle="53", sub=True,
             extra=[(102, "{ACAD_REACTORS"), (330, "60"), (102, "}")]) + [(1001, "MYAPP"), (1000, "note"), (1040, "3.5"), (1070, "7")],
        arc("0.0", "0.0", "1.0", "0.0", "90.0", extra=[(210, "0.0"), (220, "0.0"), (230, "-1.0")]))))

write("empty_values.dxf", doc(
    sec("HEADER", hdr(ACADVER=[(1, "AC1015")], TEXTSTYLE=[(7, "")], PROJECTNAME=[(1, "")])),
    sec("ENTITIES",
        [(0, "TEXT"), (8, "0"), (10, "0.0"), (20, "0.0"), (40, "1.0"), (1, "")],
        line("0.0", "0.0", "0.0", "0.0", extra=[(999, "")]))))

write("damaged_entities.dxf", doc(
    sec("HEADER", hdr(ACADVER=[(1, "AC1015")])),
    sec("ENTITIES",
        [(0, "LINE"), (8, "0"), (10, "not-a-number"), (20, "0.0"), (11, "1.0"), (21, "1.0")],
        [(0, "CIRCLE"), (8, "0"), (10, "0.0"), (20, "0.0")],
        [(0, "ARC"), (8, "0"), (10, "0.0"), (20, "0.0"), (40, "-2.0"), (50, "0"), (51, "90")],
        [(0, "CIRCLE"), (8, "0"), (62, "999"), (10, "0.0"), (20, "0.0"), (40, "1.0")],
        line("0.0", "0.0", "1.0", "1.0"))))

write("utf8_names.dxf", doc(
    sec("HEADER", hdr(ACADVER=[(1, "AC1032")], CLAYER=[(8, "Wände")])),
    sec("TABLES", table("LAYER", [(70, "3")], layer("0"), layer("Wände", color=2), layer("配管", color=4))),
    sec("ENTITIES", line("0.0", "0.0", "1.0", "0.0", "wände"), circle("0.0", "0.0", "1.0", "配管"))), trailing=False)
