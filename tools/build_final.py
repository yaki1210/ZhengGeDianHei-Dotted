# -*- coding: utf-8 -*-
"""Build the final variants from BOTH base fonts:
  - ZhengGeDianHei-16.ttf        -> full-width symbols (default)
  - ZhengGeDianHei16-Halfwidth.ttf -> half-width symbols (narrow terminals)

All 12 files share the ORIGINAL family name 'ZhengGeDianHei 16' so the
switcher can hot-swap them invisibly. Half-width files get a '-HW' suffix.
"""
import shutil
from pathlib import Path
from fontTools.ttLib import TTFont
from build_variants import SRC, DIST_DIR, extract_all, build_variant

FONTS_OUT = DIST_DIR / "fonts"
FINAL_NAME = ("ZhengGeDianHei 16", "正格点黑 16", "ZhengGeDianHei-16")
VARIANTS = [("squares", 70), ("squares", 80),
            ("dots", 70), ("dots", 80), ("dots", 90)]
BASES = [
    (SRC, ""),
    (SRC.parent / "ZhengGeDianHei16-Halfwidth.ttf", "-HW"),
]


def main():
    FONTS_OUT.mkdir(parents=True, exist_ok=True)
    ref = None
    ref_num = None
    for src, suffix in BASES:
        tag = "HW" if suffix else ""
        if not suffix:
            shutil.copy2(src, FONTS_OUT / "ZhengGeDianHei16-Original.ttf")
            print("copied original (full-width)")
        else:
            shutil.copy2(src, FONTS_OUT / "ZhengGeDianHei16-Original-HW.ttf")
            print("copied original-HW (half-width)")

        probe = TTFont(src)
        pixels = extract_all(probe)
        if ref is None:
            ref = probe.getBestCmap()
            ref_num = probe["maxp"].numGlyphs
        probe.close()

        for shape, pct in VARIANTS:
            shape_tag = "Dots" if shape == "dots" else "Squares"
            out = FONTS_OUT / f"ZhengGeDianHei16-{shape_tag}{pct}{suffix}.ttf"
            build_variant(pixels, shape, pct, out, final_name=FINAL_NAME,
                          src=src)

            # verify: same cmap/glyph count as the full-width original
            f = TTFont(out)
            assert f.getBestCmap() == ref
            assert f["maxp"].numGlyphs == ref_num
            assert f["name"].getDebugName(1) == "ZhengGeDianHei 16"
            f.close()
            print(f"  verified {out.name}")
    print("all final fonts built in", FONTS_OUT)


if __name__ == "__main__":
    main()
