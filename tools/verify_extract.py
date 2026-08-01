# -*- coding: utf-8 -*-
"""Cross-check scanline extraction against PointInsidePen on sample glyphs."""
import random
from fontTools.ttLib import TTFont
from build_variants import (SRC, XMID, YMID, J_RANGE,
                            glyph_vsegments, cells_from_vsegs)
from fontTools.pens.pointInsidePen import PointInsidePen

font = TTFont(SRC)
glyf = font["glyf"]
glyphSet = font.getGlyphSet()
cmap = font.getBestCmap()

sample_names = {cmap[c] for c in "一中汉永国愛あのアAag6。、" if c in cmap}
random.seed(42)
order = font.getGlyphOrder()
sample_names |= set(random.sample(order, 40))

mismatch = 0
checked = 0
for name in sample_names:
    glyph = glyf[name]
    if glyph.numberOfContours <= 0:
        continue
    vsegs, ok = glyph_vsegments(glyph, glyf)
    if not ok:
        continue
    fast = cells_from_vsegs(vsegs)
    width = int(glyphSet[name].width)
    i_max = max(2, int(width // 62.5) + 2)
    slow = set()
    for j in J_RANGE:
        for i in range(-2, i_max):
            pen = PointInsidePen(glyphSet, (XMID[i], YMID[j]), evenOdd=True)
            glyphSet[name].draw(pen)
            if pen.getResult():
                slow.add((i, j))
    checked += 1
    if fast != slow:
        mismatch += 1
        print(f"MISMATCH {name}: only-fast={sorted(fast - slow)[:8]} "
              f"only-pip={sorted(slow - fast)[:8]}")

print(f"checked {checked} glyphs, mismatches: {mismatch}")
for ch in "一中汉永":
    n = cmap[ord(ch)]
    v, ok = glyph_vsegments(glyf[n], glyf)
    print(ch, "cells:", len(cells_from_vsegs(v)))
