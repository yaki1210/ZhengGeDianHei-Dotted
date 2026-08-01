# -*- coding: utf-8 -*-
"""Widen Latin/Greek/Cyrillic advance widths to fix cramped letter spacing.

The original font draws all Latin/digit/punctuation glyphs in an 8-cell
advance (500 units) with ink starting at x=0 and only 1px right bearing,
so letters sit ~1px apart. This script widens those advances to 10 cells
(625 units) without touching outlines, preserving the cell-aligned grid
design (ink still starts at cell 0). Variants inherit the advance via the
existing build pipeline.

Usage:
    python fix_latin_spacing.py                  # fix base font in place
    python fix_latin_spacing.py --px 2 --src X   # custom width / source
"""
import argparse
import shutil
from pathlib import Path

from fontTools.ttLib import TTFont

CELL = 62.5
LATIN_RANGES = (
    range(0x0020, 0x007F),   # Basic Latin (incl. space, digits, punct)
    range(0x00A0, 0x0100),   # Latin-1 Supplement
    range(0x0370, 0x0400),   # Greek
    range(0x0400, 0x0500),   # Cyrillic
)
BACKUP_DIR = Path(__file__).resolve().parent / "original_backup"


def is_latin(cp, latin_only=False):
    if latin_only:
        ranges = LATIN_RANGES[:2]          # Basic Latin + Latin-1 only
    else:
        ranges = LATIN_RANGES
    return any(cp in r for r in ranges)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--px", type=int, default=2,
                    help="extra width in pixels (default 2)")
    ap.add_argument("--latin-only", action="store_true",
                    help="only Basic Latin / Latin-1 (leave Greek/Cyrillic full-width)")
    ap.add_argument("--src", type=Path,
                    default=Path(__file__).resolve().parent.parent / "ZhengGeDianHei-16.ttf")
    args = ap.parse_args()

    src = args.src.resolve()
    if not src.exists():
        raise SystemExit(f"source not found: {src}")

    BACKUP_DIR.mkdir(parents=True, exist_ok=True)
    backup = BACKUP_DIR / src.name
    if not backup.exists():
        shutil.copy2(src, backup)
        print(f"backed up original -> {backup}")

    font = TTFont(src)
    cmap = font.getBestCmap()
    hmtx = font["hmtx"]

    add = int(round(args.px * CELL))
    new_adv = int(8 * CELL) + add              # 500 -> 625 for px=2
    touched = 0
    for cp, name in cmap.items():
        if not is_latin(cp, args.latin_only):
            continue
        adv, lsb = hmtx[name]
        if adv == new_adv:
            continue
        hmtx[name] = (new_adv, lsb)
        touched += 1

    if "hdmx" in font:                            # stale device metrics
        del font["hdmx"]

    font.save(src)
    print(f"widened {touched} glyphs to advance {new_adv} -> {src}")
    print("CJK glyphs untouched (1000-unit advances).")


if __name__ == "__main__":
    main()
