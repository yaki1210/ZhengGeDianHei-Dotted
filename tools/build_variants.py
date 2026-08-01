# -*- coding: utf-8 -*-
"""
ZhengGeDianHei-16 pixel-shape modifier.

Reads the original TTF (pixels drawn as merged axis-aligned square outlines
on a 16x16 grid, cell = 62.5 font units, em = 1000), recovers the exact
bitmap of every glyph by scanline-sampling the outlines at cell centers,
then rebuilds each lit pixel as a centered circle (dots) or a scaled-down
square (gapped squares).

Usage:
    python build_variants.py            # build all preview variants
    python build_variants.py dots 80    # build a single variant
"""
import sys
from pathlib import Path

from fontTools.ttLib import TTFont
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.pens.pointInsidePen import PointInsidePen

BASE = Path(__file__).resolve().parent.parent
SRC = BASE / "ZhengGeDianHei-16.ttf"
PREVIEW_DIR = BASE / "preview"
PREVIEW_FONTS = PREVIEW_DIR / "fonts"
DIST_DIR = BASE / "dist"

CELL = 62.5                      # font units per pixel cell
GRID = [int(round(k * CELL)) for k in range(-8, 25)]   # canonical cell edges
XMID = {i: (i + 0.5) * CELL for i in range(-8, 24)}
YMID = {j: (j + 0.5) * CELL for j in range(-8, 24)}
J_RANGE = range(-3, 18)          # rows that can contain ink (y: -187..1125)

SHAPES = {"dots": "圆点", "squares": "方块"}


# ---------------------------------------------------------------- extraction
def glyph_vsegments(glyph, glyf):
    """Return (vsegs, ok): vertical segments of all contours.
    ok=False if the glyph has curves or diagonal edges (needs fallback)."""
    coords, endPts, flags = glyph.getCoordinates(glyf)
    if any(not (f & 1) for f in flags):          # off-curve point -> curves
        return None, False
    vsegs = []
    start = 0
    for end in endPts:
        pts = coords[start:end + 1]
        n = len(pts)
        for a in range(n):
            x0, y0 = pts[a]
            x1, y1 = pts[(a + 1) % n]
            if x0 == x1:
                if y0 != y1:
                    vsegs.append((x0, min(y0, y1), max(y0, y1)))
            elif y0 != y1:
                return None, False               # diagonal edge
        start = end + 1
    return vsegs, True


def cells_from_vsegs(vsegs):
    """Even-odd scanline fill at cell-center height of each row."""
    cells = set()
    for j in J_RANGE:
        y = YMID[j]
        xs = sorted(x for (x, lo, hi) in vsegs if lo < y < hi)
        for k in range(0, len(xs) - 1, 2):
            xa, xb = xs[k], xs[k + 1]
            i0 = int(xa // CELL) - 1
            i1 = int(xb // CELL) + 2
            for i in range(i0, i1):
                if i in XMID and xa < XMID[i] < xb:
                    cells.add((i, j))
    return cells


def cells_from_pip(glyphSet, name):
    """Slow but general fallback: point-in-polygon per cell center."""
    gs = glyphSet[name]
    cells = set()
    width = int(gs.width)
    i_max = max(2, int(width // CELL) + 2)
    for j in J_RANGE:
        for i in range(-2, i_max):
            pen = PointInsidePen(glyphSet, (XMID[i], YMID[j]), evenOdd=True)
            gs.draw(pen)
            if pen.getResult():
                cells.add((i, j))
    return cells


def extract_all(font):
    """{glyph_name: set((i, j), ...)} for every non-empty glyph."""
    glyf = font["glyf"]
    glyphSet = font.getGlyphSet()
    out, fallbacks = {}, 0
    for name in font.getGlyphOrder():
        glyph = glyf[name]
        if glyph.numberOfContours <= 0:
            continue
        vsegs, ok = glyph_vsegments(glyph, glyf)
        if ok:
            cells = cells_from_vsegs(vsegs)
        else:
            cells = cells_from_pip(glyphSet, name)
            fallbacks += 1
        if cells:
            out[name] = cells
    print(f"  extracted {len(out)} glyphs ({fallbacks} via fallback)")
    return out


# ------------------------------------------------------------------ rebuild
def cell_box(i, j):
    return GRID[i + 8], GRID[i + 9], GRID[j + 8], GRID[j + 9]  # xL xR yB yT


def draw_square(pen, i, j, side):
    xL, xR, yB, yT = cell_box(i, j)
    cx, cy = (xL + xR) // 2, (yB + yT) // 2
    x0, y0 = cx - side // 2, cy - side // 2
    x1, y1 = x0 + side, y0 + side
    pen.moveTo((x0, y0))
    pen.lineTo((x1, y0))
    pen.lineTo((x1, y1))
    pen.lineTo((x0, y1))
    pen.closePath()


def draw_dot(pen, i, j, side):
    xL, xR, yB, yT = cell_box(i, j)
    cx, cy = (xL + xR) // 2, (yB + yT) // 2
    rx = ry = max(1, side // 2)
    pen.moveTo((cx + rx, cy))
    pen.qCurveTo((cx + rx, cy + ry), (cx, cy + ry))
    pen.qCurveTo((cx - rx, cy + ry), (cx - rx, cy))
    pen.qCurveTo((cx - rx, cy - ry), (cx, cy - ry))
    pen.qCurveTo((cx + rx, cy - ry), (cx + rx, cy))
    pen.closePath()


def rename(font, family_en, family_zh, ps_name):
    name = font["name"]
    full_en, full_zh = family_en, family_zh
    for nid, en, zh in ((1, family_en, family_zh), (4, full_en, full_zh),
                        (6, ps_name, ps_name), (16, family_en, family_zh)):
        name.setName(en, nid, 3, 1, 0x409)
        name.setName(zh, nid, 3, 1, 0x804)
        name.setName(en, nid, 1, 0, 0)
    import datetime
    stamp = datetime.date.today().isoformat()
    name.setName(f"{ps_name}; dot-mod {stamp}", 3, 3, 1, 0x409)


def build_variant(pixels_by_glyph, shape, pct, out_path, final_name=None,
                  src=SRC):
    font = TTFont(src)
    glyf = font["glyf"]
    hmtx = font["hmtx"]
    side = int(round(CELL * pct / 100))
    draw = draw_dot if shape == "dots" else draw_square

    for name in font.getGlyphOrder():
        cells = pixels_by_glyph.get(name)
        if not cells:
            continue
        pen = TTGlyphPen(None)
        for (i, j) in sorted(cells):
            draw(pen, i, j, side)
        g = pen.glyph()
        g.recalcBounds(glyf)
        glyf[name] = g
        adv, _ = hmtx[name]
        hmtx[name] = (adv, g.xMin)

    # metadata -----------------------------------------------------------
    pct_s = str(pct)
    if final_name:
        fam_en, fam_zh, ps = final_name
    else:
        fam_en = f"ZhengGeDianHei 16 {'Dots' if shape == 'dots' else 'Squares'} {pct_s}"
        fam_zh = f"正格点黑 16 {SHAPES[shape]} {pct_s}"
        ps = f"ZhengGeDianHei16-{'Dots' if shape == 'dots' else 'Squares'}{pct_s}"
    rename(font, fam_en, fam_zh, ps)

    for tag in ("bdat", "bloc"):           # drop old embedded bitmaps
        if tag in font:
            del font[tag]
    if "gasp" in font:                     # grayscale AA, no gridfit
        font["gasp"].gaspRange = {65535: 0x0002}

    font.save(out_path)
    print(f"  saved {out_path.name}  ({out_path.stat().st_size // 1024} KB)")


# -------------------------------------------------------------------- main
def main():
    args = sys.argv[1:]
    single = None
    if len(args) == 2:
        single = (args[0], int(args[1]))

    PREVIEW_FONTS.mkdir(parents=True, exist_ok=True)
    sources = [SRC, SRC.parent / "ZhengGeDianHei16-Halfwidth.ttf"]
    for src in sources:
        hw = "HW" if src.name.startswith("ZhengGeDianHei16-Halfwidth") else ""
        print(f"extracting pixel data from {src.name} ...")
        probe = TTFont(src)
        pixels = extract_all(probe)
        probe.close()
        variants = [(s, p) for s in ("dots", "squares") for p in (70, 80, 90)]
        for shape, pct in variants:
            if single and (shape, pct) != single:
                continue
            tag = "Dots" if shape == "dots" else "Squares"
            out = PREVIEW_FONTS / f"ZhengGeDianHei16-{tag}{pct}{'-' + hw if hw else ''}.ttf"
            build_variant(pixels, shape, pct, out, src=src)
    print("done.")


if __name__ == "__main__":
    main()
