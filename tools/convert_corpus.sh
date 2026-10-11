#!/usr/bin/env bash
# Re-saves each ezdxf source drawing through every installed DXF writer.
#
# Usage: tools/convert_corpus.sh SOURCE_DIR OUT_DIR [PRIVATE_DIR]
#
# SOURCE_DIR holds <drawing>__ezdxf__R2018.dxf files (from gen_sources.py).
# Outputs are written as OUT_DIR/<drawing>__<tool>__<version>.dxf, and a
# tab-separated log of every conversion (ok or failed) to OUT_DIR/convert.log.
# Files written by ODA File Converter, and by LibreDWG from an ODA-made DWG,
# go to PRIVATE_DIR instead (default: tests/corpus-private/converted next to
# OUT_DIR), which is git-ignored: they are not part of the public repository.
#
# Tools used when present:
#   oda       ODA File Converter (~/Applications or /Applications)
#   freecad   freecadcmd (FreeCAD.app): import, then export (FreeCAD writes R14)
#   libredwg  dwg2dxf in each version, from a DWG made by LibreDWG's own dxf2dwg
#             or, when that can't read the source (its DXF reader is
#             experimental), from a DWG 2018 made by ODA
set -u

SRC=${1:?usage: convert_corpus.sh SOURCE_DIR OUT_DIR [PRIVATE_DIR]}
OUT=${2:?usage: convert_corpus.sh SOURCE_DIR OUT_DIR [PRIVATE_DIR]}
PRIVATE=${3:-$(dirname "$OUT")/corpus-private/converted}
HERE=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$OUT" "$PRIVATE"
LOG="$OUT/convert.log"
: > "$LOG"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

# Runs a command with a time limit (seconds); macOS has no `timeout`.
limit() { perl -e 'alarm shift; exec @ARGV' "$@"; }

log() { printf '%s\t%s\t%s\t%s\n' "$1" "$2" "$3" "$4" >> "$LOG"; }

SOURCES=("$SRC"/*__ezdxf__R2018.dxf)
drawing_of() { basename "$1" | sed 's/__ezdxf__R2018\.dxf$//'; }

# Names of the sections in a DXF file, comma-separated.
sections() { tr -d '\r' < "$1" | awk 'prev == "SECTION" && $1 == "2" { getline; printf "%s%s", sep, $1; sep = "," } { prev = $1 }'; }

# Accept a converted file only if it is a complete ASCII DXF: starts with a
# SECTION and ends with EOF (some writers silently stop part-way).
accept() { # accept TMPFILE DRAWING TOOL VERSION
    local f=$1 dir=$OUT
    case $3 in oda | libredwg) dir=$PRIVATE ;; esac
    local dest="$dir/$2__$3__$4.dxf"
    if [ ! -s "$f" ] || ! head -c 200 "$f" | grep -q 'SECTION'; then
        log "$2" "$3" "$4" "failed: no usable output"
    elif ! tail -c 64 "$f" | tr -d ' \r\n' | grep -q 'EOF$'; then
        log "$2" "$3" "$4" "failed: truncated output, no EOF (sections: $(sections "$f"))"
    else
        mv "$f" "$dest"
        log "$2" "$3" "$4" "ok${ROUTE:+ ($ROUTE)}"
    fi
}

# --- ODA File Converter ---------------------------------------------------
ROUTE=""
ODA=""
for app in "$HOME/Applications" /Applications; do
    [ -x "$app/ODAFileConverter.app/Contents/MacOS/ODAFileConverter" ] &&
        ODA="$app/ODAFileConverter.app/Contents/MacOS/ODAFileConverter" && break
done
if [ -n "$ODA" ]; then
    mkdir -p "$TMP/oda_in"
    for s in "${SOURCES[@]}"; do cp "$s" "$TMP/oda_in/$(drawing_of "$s").dxf"; done
    for pair in ACAD12:R12 ACAD2000:R2000 ACAD2004:R2004 ACAD2007:R2007 \
                ACAD2010:R2010 ACAD2013:R2013 ACAD2018:R2018; do
        oda_ver=${pair%%:*} label=${pair##*:}
        mkdir -p "$TMP/oda_$label"
        # Args: in-dir out-dir version type recursive audit filter
        limit 600 "$ODA" "$TMP/oda_in" "$TMP/oda_$label" "$oda_ver" DXF 0 1 "*.dxf" \
            > "$TMP/oda_$label.log" 2>&1
        for s in "${SOURCES[@]}"; do
            d=$(drawing_of "$s")
            [ -f "$TMP/oda_$label/$d.dxf.err" ] &&
                log "$d" oda "$label" "error report: $(tr '\n' ' ' < "$TMP/oda_$label/$d.dxf.err" | sed "s|$TMP|<tmp>|g" | cut -c1-150)"
            accept "$TMP/oda_$label/$d.dxf" "$d" oda "$label"
        done
    done
    # DWG copies, used as LibreDWG's input when its DXF reader fails.
    mkdir -p "$TMP/oda_dwg"
    limit 600 "$ODA" "$TMP/oda_in" "$TMP/oda_dwg" ACAD2018 DWG 0 1 "*.dxf" \
        > "$TMP/oda_dwg.log" 2>&1
else
    echo "oda: not installed, skipped" | tee -a "$LOG"
fi

# --- FreeCAD ----------------------------------------------------------------
FREECADCMD=""
for app in /Applications "$HOME/Applications"; do
    for exe in "$app/FreeCAD.app/Contents/Resources/bin/freecadcmd" \
               "$app/FreeCAD.app/Contents/MacOS/FreeCADCmd"; do
        [ -x "$exe" ] && FREECADCMD=$exe && break 2
    done
done
if [ -n "$FREECADCMD" ]; then
    for s in "${SOURCES[@]}"; do
        d=$(drawing_of "$s")
        limit 300 "$FREECADCMD" "$HERE/freecad_resave.py" --pass "$s" "$TMP/fc_$d.dxf" \
            > "$TMP/fc_$d.log" 2>&1
        accept "$TMP/fc_$d.dxf" "$d" freecad R14
    done
else
    echo "freecad: not installed, skipped" | tee -a "$LOG"
fi

# --- LibreDWG ---------------------------------------------------------------
PATH="$HOME/.local/bin:$PATH"  # where a from-source build installs
if command -v dxf2dwg > /dev/null && command -v dwg2dxf > /dev/null; then
    for s in "${SOURCES[@]}"; do
        d=$(drawing_of "$s")
        dwg="$TMP/ld_$d.dwg"
        ROUTE="DXF->DWG by dxf2dwg"
        if ! limit 120 dxf2dwg -y -o "$dwg" "$s" > "$TMP/ld_$d.log" 2>&1 || [ ! -s "$dwg" ]; then
            dwg="$TMP/oda_dwg/$d.dwg"
            ROUTE="DXF->DWG by ODA"
            if [ ! -s "$dwg" ]; then
                log "$d" libredwg "*" "failed: dxf2dwg could not read the source and no ODA DWG"
                continue
            fi
        fi
        for ver in r12 r14 r2000 r2004 r2007 r2010 r2013 r2018; do
            label=$(echo "$ver" | tr r R)
            limit 120 dwg2dxf -y --as "$ver" -o "$TMP/ld_${d}_$ver.dxf" "$dwg" \
                >> "$TMP/ld_$d.log" 2>&1
            accept "$TMP/ld_${d}_$ver.dxf" "$d" libredwg "$label"
        done
    done
    ROUTE=""
else
    echo "libredwg: not installed, skipped" | tee -a "$LOG"
fi

echo "done: $(grep -c $'\tok' "$LOG") files ok, $(grep -vc $'\tok' "$LOG") other log lines; see $LOG"
