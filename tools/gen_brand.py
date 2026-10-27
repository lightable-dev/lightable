"""Lightable's brand kit (docs/brand/): the logo mark, the wordmark (Lato
outlined, so it needs no font), the lockup, and the font files with their
licence. Everything here is original except Lato (SIL Open Font Licence
1.1, unmodified).

Usage: tools/.venv/bin/python tools/gen_brand.py LATO_DIR OFL_TEMPLATE

LATO_DIR holds Lato-Regular.ttf and Lato-Light.ttf (version 1.105, as
shipped with RDoc's darkfish template); OFL_TEMPLATE is any copy of the
OFL 1.1 text (its first paragraph, the copyright line, is replaced with
Lato's own, read from the font).
"""

import shutil
import sys
from pathlib import Path

from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.ttLib import TTFont

OUT = Path(__file__).resolve().parent.parent / "docs" / "brand"
FONTS = OUT / "fonts"

# The palette: the app's own change colours, on a light-table night.
INK = "#0b0f14"
PAPER = "#eef2f6"
BLUE = "#4cb0ff"  # the app's "added"
YELLOW = "#f5d02a"  # the app's "changed"
GLOW = "#ffffff"


def mark(size=64):
    """Two sheets on a light table: the earlier one (blue), the later one
    (yellow), and where they overlap the light shines through (white)."""
    return f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="{size}" height="{size}">
  <title>Lightable</title>
  <rect x="6" y="8" width="36" height="42" rx="6" fill="{BLUE}"/>
  <rect x="22" y="16" width="36" height="42" rx="6" fill="{YELLOW}"/>
  <path d="M22 22a6 6 0 0 1 6-6h14v28a6 6 0 0 1-6 6H22z" fill="{GLOW}"/>
</svg>
"""


def outline(font_path, text, size, tracking=0.02):
    """The text as one SVG path (y down), and its advance width."""
    font = TTFont(font_path)
    glyphs = font.getGlyphSet()
    cmap = font.getBestCmap()
    scale = size / font["head"].unitsPerEm
    ascent = font["hhea"].ascent * scale
    x = 0.0
    parts = []
    for ch in text:
        name = cmap[ord(ch)]
        pen = SVGPathPen(glyphs)
        glyphs[name].draw(TransformPen(pen, (scale, 0, 0, -scale, x, ascent)))
        parts.append(pen.getCommands())
        x += glyphs[name].width * scale + tracking * size
    return " ".join(parts), x - tracking * size, ascent


def main():
    lato, ofl = Path(sys.argv[1]), Path(sys.argv[2])
    OUT.mkdir(parents=True, exist_ok=True)
    FONTS.mkdir(parents=True, exist_ok=True)
    for name in ["Lato-Regular.ttf", "Lato-Light.ttf"]:
        shutil.copyfile(lato / name, FONTS / name)
    copyright = next(r.toUnicode() for r in TTFont(lato / "Lato-Regular.ttf")["name"].names if r.nameID == 0)
    text = ofl.read_text()
    body = text[text.index("This Font Software is licensed"):]
    (FONTS / "OFL.txt").write_text(copyright + "\n\n" + body)

    (OUT / "logo-mark.svg").write_text(mark())
    path, width, ascent = outline(lato / "Lato-Regular.ttf", "lightable", 48)
    height = 48 * 1.2
    (OUT / "wordmark.svg").write_text(
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width:.1f} {height:.1f}" '
        f'width="{width:.0f}" height="{height:.0f}">\n  <title>Lightable</title>\n'
        f'  <path d="{path}" fill="{PAPER}"/>\n</svg>\n'
    )
    # Lockup: the mark, then the wordmark at the mark's height.
    gap = 14
    (OUT / "lockup.svg").write_text(
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {64 + gap + width:.1f} 64" '
        f'width="{64 + gap + width:.0f}" height="64">\n  <title>Lightable</title>\n'
        + "\n".join("  " + line for line in mark().splitlines()[2:-1])
        + f'\n  <g transform="translate({64 + gap} {32 - (ascent * 0.72) / 2 - ascent * 0.28:.1f})">'
        f'<path d="{path}" fill="{PAPER}"/></g>\n</svg>\n'
    )


if __name__ == "__main__":
    main()
