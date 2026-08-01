# -*- coding: utf-8 -*-
"""Build the half-width compat font: symbols squeezed to fit one terminal cell.

Terminals allocate cells by Unicode width (wcwidth), not by font metrics.
Glyphs whose codepoint is narrow/ambiguous (EAW=N/NA/A) get only 1 cell
(8px = 500 units of ink) in narrow-mode terminals; any ink wider than that
overlaps the next character. This script rebuilds such glyphs to fit:

  letters        (Unicode L*, Nl: Greek/Cyrillic, roman numerals, ...)
                 -> x compressed to 8 cells (same width as Latin), height kept
  tall symbols   (aspect >= 1.4: sqrt, summation, element, ...)
                 -> x halved, height kept
  wide symbols   (aspect <  0.7: -> <-> infinity, ...)
                 -> x halved, height kept
  square symbols (0.7 <= aspect < 1.4: filledbox, heart, diamond, circles, ...)
                 -> x AND y halved, vertically centered (proportional 8x8-ish)

All converted glyphs get advance 625 (10 cells), like the Latin letters.
CJK ideographs / fullwidth forms / box drawing / block elements stay put.

Reads the full-width base, writes ZhengGeDianHei16-Halfwidth.ttf; the base
is never modified, so the script is safe to re-run.

Usage:
    python fix_halfwidth_symbols.py
"""
import sys
import unicodedata
from pathlib import Path

from fontTools.ttLib import TTFont
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.pens.pointInsidePen import PointInsidePen

sys.path.insert(0, str(Path(__file__).resolve().parent))
from build_variants import CELL, GRID, XMID, YMID, J_RANGE, \
    glyph_vsegments, cells_from_vsegs

BASE = Path(__file__).resolve().parent.parent / "ZhengGeDianHei-16.ttf"
OUT = Path(__file__).resolve().parent.parent / "ZhengGeDianHei16-Halfwidth.ttf"

EXCLUDE = (
    (0x2E80, 0x9FFF),     # CJK ideographs
    (0xF900, 0xFAFF),     # CJK compat ideographs
    (0xFF00, 0xFFEF),     # fullwidth forms (EAW=F, terminal: 2 cells)
    (0x2500, 0x257F),     # box drawing (TUI borders, must connect)
    (0x2580, 0x259F),     # block elements (progress bars etc.)
    (0x20000, 0x3FFFF),   # CJK ext B+ (astral CJK)
)

# Filled/rounded shapes: halving only x makes them look stretched
# (flat bars / ovals). These get a proportional 8x8-ish rebuild instead.
PROPORTIONAL = (
    (0x25A0, 0x25EF),     # geometric shapes (squares, triangles, circles)
    (0x2460, 0x24FF),     # circled digits / letters ①..㉒ⓐ..
    (0x2600, 0x26FF),     # misc symbols (sun, cloud, phone, warning...)
    (0x2660, 0x266F),     # card suits ♠♣♥♦♡♢
    (0x2776, 0x277E),     # dingbat circled digits ❶..❾
    (0x27A0, 0x27BF),     # heavy arrows ➡ etc.
    (0x2B00, 0x2BFF),     # arrows with tails / stars ⬅⬆⬇
    (0x21BA, 0x21BB),     # circular arrows ↺↻
    (0x1F100, 0x1F100),   # 0 in a circle
)

LETTER_CATS = ("Lu", "Ll", "Lt", "Lm", "Lo", "Nl")
ADV = int(10 * CELL)                      # 625, matches the Latin letters


def excluded(cp):
    return any(a <= cp <= b for a, b in EXCLUDE)


def proportional(cp):
    return any(a <= cp <= b for a, b in PROPORTIONAL)


def draw_full_box(pen, i, j):
    xL, xR, yB, yT = GRID[i + 8], GRID[i + 9], GRID[j + 8], GRID[j + 9]
    pen.moveTo((xL, yB))
    pen.lineTo((xR, yB))
    pen.lineTo((xR, yT))
    pen.lineTo((xL, yT))
    pen.closePath()


def cells_of(glyf, glyphSet, name):
    glyph = glyf[name]
    vsegs, ok = glyph_vsegments(glyph, glyf)
    if ok:
        return cells_from_vsegs(vsegs)
    cells = set()
    width = int(glyphSet[name].width)
    i_max = max(2, int(width // CELL) + 2)
    for j in J_RANGE:
        for i in range(-2, i_max):
            pen = PointInsidePen(glyphSet, (XMID[i], YMID[j]), evenOdd=True)
            glyphSet[name].draw(pen)
            if pen.getResult():
                cells.add((i, j))
    return cells


def rebuild(font, cells, shift_j=0):
    pen = TTGlyphPen(None)
    for i, j in sorted(cells):
        draw_full_box(pen, i, j + shift_j)
    g = pen.glyph()
    g.recalcBounds(font["glyf"])
    return g


def main():
    font = TTFont(BASE)
    glyf = font["glyf"]
    hmtx = font["hmtx"]
    cmap = font.getBestCmap()
    glyphSet = font.getGlyphSet()

    stats = {"letters": 0, "tall": 0, "square": 0}
    for cp, name in sorted(cmap.items()):
        g = glyf[name]
        if g.numberOfContours <= 0:
            continue
        adv, _ = hmtx[name]
        ink_w = g.xMax - g.xMin
        overflows = g.xMin < 0 or g.xMax > adv
        if not overflows and ink_w <= 500:
            continue                    # already fits one cell, keep as is
        if excluded(cp):
            continue

        cells = cells_of(glyf, glyphSet, name)
        if not cells:
            continue
        xs = [i for i, j in cells]
        ys = [j for i, j in cells]
        i_min, i_max = min(xs), max(xs)
        w = i_max - i_min + 1

        ch = chr(cp) if cp < 0x10000 else ""
        is_letter = bool(ch) and unicodedata.category(ch) in LETTER_CATS
        if is_letter:
            # letters: squeeze to 8 cells wide (match Latin), height kept
            new = set()
            for i, j in cells:
                new.add(((i - i_min) * 8 // w, j))
            stats["letters"] += 1
        elif proportional(cp):
            # filled/rounded shapes: halve both axes, center vertically
            j_min = min(ys)
            new = set()
            for i, j in cells:
                new.add(((i - i_min) // 2, (j - j_min) // 2))
            jj = [j for i, j in new]
            shift = 8 - (max(jj) + min(jj)) // 2
            new = {(i, j + shift) for i, j in new}
            stats["square"] += 1
        else:
            # thin-stroke symbols (sqrt, sum, arrows, ...): halve x only
            if w > 8:
                new = set()
                for i, j in cells:
                    new.add(((i - i_min) // 2, j))
            else:
                new = set()
                for i, j in cells:
                    new.add((i - i_min, j))
            stats["tall"] += 1

        newg = rebuild(font, new)
        glyf[name] = newg
        hmtx[name] = (ADV, newg.xMin)

    if "hdmx" in font:
        del font["hdmx"]

    font.save(OUT)
    print("half-width font saved ->", OUT)
    print("converted:", stats, "total:", sum(stats.values()))

    # verification: nothing may overflow its advance
    bad = 0
    for cp, name in cmap.items():
        g = glyf[name]
        if g.numberOfContours > 0:
            adv, _ = hmtx[name]
            if g.xMin < 0 or g.xMax > adv:
                print(f"  still overflowing: U+{cp:04X} {name} adv={adv} "
                      f"ink={g.xMin}..{g.xMax}")
                bad += 1
    print("remaining overflows:", bad)


if __name__ == "__main__":
    main()
