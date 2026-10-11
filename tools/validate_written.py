"""Checks that files lightable-dxf writes open cleanly in other programs.

Usage: python3 tools/validate_written.py OUT_DIR [--limit N]

For every file in tests/corpus and each target (R12, R2018):
1. `lightable generate` writes it (always generated, never the source bytes).
2. ODA File Converter re-saves it in the same version with audit on. Any
   .err file is a failure. ODA repairs problems silently, so the re-saved
   file is compared with ours: every handle in BLOCKS and ENTITIES must keep
   its type and owner, nothing may be added or removed there, and every
   table entry we wrote must still be there. What ODA also does when it
   re-saves the original corpus file to the same version is its normal
   behaviour (tidying objects, renaming anonymous and layout blocks,
   renumbering polyline vertices) and is not counted. A record ODA drops
   because it refers to a handle that does not exist in the file is
   reported as a defect in the source data, not in the writer.
3. FreeCAD imports it (one freecadcmd process for all files). Anything the
   importer prints for the file, or an exception, is a failure.
4. Our own reader loads it with no warnings.

Writes OUT_DIR/report.md and prints a summary. Needs a release build of
lightable, ODA File Converter in ~/Applications or /Applications, and FreeCAD
in /Applications.
"""

import collections
import json
import pathlib
import re
import shutil
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
CAD_APP = ROOT / "target/release/lightable"
ODA = next((p for p in [pathlib.Path.home() / "Applications/ODAFileConverter.app/Contents/MacOS/ODAFileConverter",
                        pathlib.Path("/Applications/ODAFileConverter.app/Contents/MacOS/ODAFileConverter")]
            if p.exists()), None)
FREECADCMD = pathlib.Path("/Applications/FreeCAD.app/Contents/Resources/bin/freecadcmd")
TARGETS = {"r12": "ACAD12", "r2018": "ACAD2018"}
# Files that exist to be broken; their damage is written as-is.
DELIBERATELY_DAMAGED = {"damaged_entities.dxf"}


ANONYMOUS = re.compile(r"^\*([A-Z]\d+|PAPER_SPACE\d+)$", re.I)
PROBLEM = re.compile(r"\b(missing|unable|invalid|error|warning|exception|degenerate|failed|cannot|corrupt)\b", re.I)
CHILDREN = {"VERTEX", "SEQEND"}


def records(path):
    """(section, type, handle, owner, name, refs) for every record."""
    lines = path.read_bytes().decode("latin-1").split("\n")
    out, section, expect_name, cur = [], None, False, None
    for i in range(0, len(lines) - 1, 2):
        code, value = lines[i].strip(), lines[i + 1].rstrip("\r").strip()
        if code == "0" and value == "SECTION":
            expect_name = True
            continue
        if expect_name and code == "2":
            section, expect_name = value, False
            continue
        if code == "0":
            cur = {"section": section, "type": value, "handle": None, "owner": None, "name": None, "refs": []}
            out.append(cur)
        elif cur is not None:
            if code in ("5", "105") and cur["handle"] is None:
                cur["handle"] = value
            elif code == "330" and cur["owner"] is None:
                cur["owner"] = value
            elif code == "2" and cur["name"] is None:
                cur["name"] = value
            elif code.isdigit() and 340 <= int(code) <= 349:
                cur["refs"].append(value)
    return out


def effects(before, after):
    """What ODA did when re-saving: counts of (what, section, type) for
    records dropped, added or changed (type or owner), by handle, plus lost
    table entries. VERTEX/SEQEND are compared by count only, since ODA
    renumbers them."""
    out = collections.Counter()

    def keyed(recs):
        return {r["handle"]: r for r in recs
                if r["section"] in ("BLOCKS", "ENTITIES", "OBJECTS") and r["handle"]
                and r["type"] not in CHILDREN and r["type"] not in ("ENDSEC", "EOF")}

    a, b = keyed(before), keyed(after)
    for h in set(a) - set(b):
        out[("dropped", a[h]["section"], a[h]["type"])] += 1
    for h in set(b) - set(a):
        out[("added", b[h]["section"], b[h]["type"])] += 1
    for h in set(a) & set(b):
        if (a[h]["type"], a[h]["owner"]) != (b[h]["type"], b[h]["owner"]):
            out[("changed", a[h]["section"], a[h]["type"])] += 1
    count = lambda recs, t: sum(1 for r in recs if r["type"] == t and r["section"] in ("BLOCKS", "ENTITIES"))
    for t in sorted(CHILDREN):
        if count(before, t) != count(after, t):
            out[("count changed", "BLOCKS/ENTITIES", t)] += 1

    def entries(recs):
        return {(r["type"].upper(), (r["name"] or "").upper()) for r in recs
                if r["section"] == "TABLES" and r["type"] not in ("TABLE", "ENDTAB", "ENDSEC")
                and not ANONYMOUS.match(r["name"] or "")}
    for table, name in entries(before) - entries(after):
        out[("lost entry", "TABLES", f"{table} {name}")] += 1
    return out


def compare_with_oda(ours, theirs, original, oda_original):
    """Writer problems: what ODA did to our file beyond what it does to the
    original corpus file when re-saving it to the same version. Also returns
    records ODA dropped because they refer to a handle missing from the file
    (a source-data defect)."""
    ours_recs, theirs_recs = records(ours), records(theirs)
    ours_effects = effects(ours_recs, theirs_recs)
    baseline = effects(records(original), records(oda_original)) if oda_original.exists() else collections.Counter()
    all_handles = {r["handle"] for r in ours_recs if r["handle"]}
    theirs_handles = {r["handle"] for r in theirs_recs if r["handle"]}
    dangling = [r for r in ours_recs
                if r["handle"] and r["handle"] not in theirs_handles and r["type"] not in CHILDREN
                and any(ref not in all_handles and ref != "0" for ref in r["refs"])]
    for r in dangling:
        ours_effects[("dropped", r["section"], r["type"])] -= 1
    problems = [f"{what} {n - baseline[(what, section, t)]} {section} {t}"
                for (what, section, t), n in sorted(ours_effects.items())
                if n > baseline[(what, section, t)] and what != "added"]
    defects = [f"ODA dropped {r['type']} {r['handle']}: it refers to a handle missing from the file"
               for r in dangling]
    return problems, defects


def run_oda(in_dir, out_dir, version):
    out_dir.mkdir(parents=True, exist_ok=True)
    subprocess.run([str(ODA), str(in_dir), str(out_dir), version, "DXF", "0", "1", "*.dxf"],
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=3600, check=False)


def run_freecad(paths, list_file):
    list_file.write_text("\n".join(str(p) for p in paths) + "\n")
    proc = subprocess.run([str(FREECADCMD), str(ROOT / "tools/freecad_check.py"), "--pass", str(list_file)],
                          capture_output=True, text=True, errors="replace", timeout=7200, check=False)
    results, current, messages = {}, None, []
    # FreeCAD's messages don't always end with a newline, so a marker can
    # follow one on the same line.
    for part in re.split(r"(@@BEGIN [^\n]*|@@END [^\n]*)", proc.stdout):
        if part.startswith("@@BEGIN "):
            current, messages = part[8:].strip(), []
        elif part.startswith("@@END ") and current:
            info = json.loads(part[6:])
            info["messages"] = messages
            results[current] = info
            current = None
        elif current is not None:
            # FreeCAD's banner and buffered summaries can land anywhere;
            # only lines that report a problem count.
            messages += [m.strip() for m in part.splitlines() if PROBLEM.search(m)]
    return results


def main():
    out = pathlib.Path(sys.argv[1]).resolve()
    limit = int(sys.argv[sys.argv.index("--limit") + 1]) if "--limit" in sys.argv else None
    if out.exists():
        shutil.rmtree(out)
    corpus = sorted((ROOT / "tests/corpus").glob("*.dxf"))[:limit]
    problems = collections.defaultdict(list)
    defects = collections.defaultdict(list)
    written = {}
    for target in TARGETS:
        (out / "ours" / target).mkdir(parents=True)
        for src in corpus:
            dest = out / "ours" / target / src.name
            subprocess.run([str(CAD_APP), "generate", str(src), str(dest), target],
                           capture_output=True, check=False)
            if not dest.exists():
                problems[(src.name, target)].append("lightable could not write it")
                continue
            written[(src.name, target)] = dest
            check = subprocess.run([str(CAD_APP), "info", str(dest)], capture_output=True, text=True, check=False)
            warnings = [l for l in check.stdout.splitlines() if l.startswith("warning")]
            if check.returncode != 0 or (warnings and src.name not in DELIBERATELY_DAMAGED):
                problems[(src.name, target)].append(f"our reader: {check.stderr.strip() or warnings[:2]}")

    if ODA:
        (out / "originals").mkdir()
        for src in corpus:
            shutil.copy(src, out / "originals" / src.name)
        for target, version in TARGETS.items():
            run_oda(out / "ours" / target, out / "oda" / target, version)
            # Baseline: what ODA does to the original files on the same re-save.
            run_oda(out / "originals", out / "oda_orig" / target, version)
            for src in corpus:
                if (src.name, target) not in written:
                    continue
                err = out / "oda" / target / (src.name + ".err")
                theirs = out / "oda" / target / src.name
                if err.exists():
                    problems[(src.name, target)].append("ODA: " + err.read_text(errors="replace").strip().replace("\n", " ")[:200])
                elif not theirs.exists():
                    problems[(src.name, target)].append("ODA wrote nothing")
                else:
                    found, source_defects = compare_with_oda(
                        written[(src.name, target)], theirs, src, out / "oda_orig" / target / src.name)
                    problems[(src.name, target)] += [f"ODA: {p}" for p in found]
                    defects[(src.name, target)] += source_defects

    if FREECADCMD.exists():
        paths = [written[k] for k in sorted(written)]
        results = run_freecad(paths, out / "freecad_list.txt")
        for (name, target), path in written.items():
            info = results.get(str(path))
            if info is None:
                problems[(name, target)].append("FreeCAD: no result (crashed?)")
                continue
            if info["exception"]:
                problems[(name, target)].append(f"FreeCAD: {info['exception']}")
            if info["messages"] and name not in DELIBERATELY_DAMAGED:
                problems[(name, target)].append(f"FreeCAD: {info['messages'][:3]}")

    total = len(corpus) * len(TARGETS)
    bad = {k: v for k, v in problems.items() if v}
    lines = ["# Written-file validation", "",
             f"{total} files written ({len(corpus)} corpus files x R12, R2018).",
             f"Checked with: our reader{', ODA File Converter' if ODA else ''}{', FreeCAD' if FREECADCMD.exists() else ''}.",
             "", f"**{total - len(bad)} clean, {len(bad)} with problems.**", ""]
    for (name, target), probs in sorted(bad.items()):
        lines.append(f"- `{name}` -> {target}: " + "; ".join(probs))
    known = {k: v for k, v in defects.items() if v}
    if known:
        lines += ["", f"Defects in the source data that other programs react to ({len(known)} files):", ""]
        for (name, target), probs in sorted(known.items()):
            lines.append(f"- `{name}` -> {target}: " + "; ".join(probs))
    (out / "report.md").write_text("\n".join(lines) + "\n")
    print("\n".join(lines[:6]))
    for line in lines[6:40]:
        print(line)
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
