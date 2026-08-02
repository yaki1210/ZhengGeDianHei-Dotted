# -*- coding: utf-8 -*-
"""Render comparison previews: original vs dot/square variants."""
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont

BASE = Path(__file__).resolve().parent.parent
SRC = BASE / "ZhengGeDianHei-16.ttf"
HW = BASE / "ZhengGeDianHei16-Halfwidth.ttf"
PREVIEW = BASE / "preview"
FONTS = PREVIEW / "fonts"

SAMPLE = "正格点黑16 方其中，圆其外 ←→↔⇒∞√∑≤∈■◆♥●①αβЖ Aag。、"
SHORT = "正格点黑永国←→■♥①αЖ"

ROWS = [("原版 Original", SRC),
        ("原版·半宽 Original-HW", HW)]
for p in (70, 80, 90):
    ROWS.append((f"圆点 Dots {p}%", FONTS / f"ZhengGeDianHei16-Dots{p}.ttf"))
for p in (70, 80, 90):
    ROWS.append((f"方块 Squares {p}%", FONTS / f"ZhengGeDianHei16-Squares{p}.ttf"))
for p in (70, 80, 90):
    ROWS.append((f"圆点·半宽 Dots {p}% HW", FONTS / f"ZhengGeDianHei16-Dots{p}-HW.ttf"))
for p in (70, 80, 90):
    ROWS.append((f"方块·半宽 Squares {p}% HW", FONTS / f"ZhengGeDianHei16-Squares{p}-HW.ttf"))


def label_font(size):
    for cand in ("C:/Windows/Fonts/msyh.ttc", "C:/Windows/Fonts/simsun.ttc"):
        if Path(cand).exists():
            return ImageFont.truetype(cand, size)
    return ImageFont.load_default()


def render_16px():
    size, row_h, lab_w, pad = 16, 30, 150, 16
    lf = label_font(13)
    fonts = [ImageFont.truetype(str(p), size) for _, p in ROWS]
    w = lab_w + 16 * len(SAMPLE) + 40
    img = Image.new("RGB", (w, row_h * len(ROWS) + pad * 2), "white")
    d = ImageDraw.Draw(img)
    for r, ((label, _), f) in enumerate(zip(ROWS, fonts)):
        y = pad + r * row_h
        d.text((8, y + 6), label, font=lf, fill=(90, 90, 90))
        d.text((lab_w, y), SAMPLE, font=f, fill=(0, 0, 0))
        d.line((0, y + row_h - 4, w, y + row_h - 4), fill=(230, 230, 230))
    img.save(PREVIEW / "preview_16px.png")
    print("preview_16px.png", img.size)


def render_zoom(factor=5):
    size, lab_w, pad = 16, 150, 20
    lf = label_font(14)
    cell = size * factor
    row_h = cell + 26
    w = lab_w + cell * len(SHORT) + pad * 2
    img = Image.new("RGB", (w, row_h * len(ROWS) + pad), "white")
    d = ImageDraw.Draw(img)
    for r, (label, path) in enumerate(ROWS):
        f = ImageFont.truetype(str(path), size)
        tile = Image.new("RGB", (size * len(SHORT), size + 8), "white")
        ImageDraw.Draw(tile).text((0, 0), SHORT, font=f, fill=(0, 0, 0))
        tile = tile.resize((tile.width * factor, tile.height * factor),
                           Image.NEAREST)
        y = pad + r * row_h
        img.paste(tile, (lab_w, y))
        d.text((8, y + cell // 2 - 8), label, font=lf, fill=(90, 90, 90))
    img.save(PREVIEW / "preview_zoom5x.png")
    print("preview_zoom5x.png", img.size)


def render_30px():
    size, row_h, lab_w, pad = 30, 46, 150, 16
    lf = label_font(14)
    fonts = [ImageFont.truetype(str(p), size) for _, p in ROWS]
    w = lab_w + size * len(SAMPLE) + 40
    img = Image.new("RGB", (w, row_h * len(ROWS) + pad * 2), "white")
    d = ImageDraw.Draw(img)
    for r, ((label, _), f) in enumerate(zip(ROWS, fonts)):
        y = pad + r * row_h
        d.text((8, y + size // 2 - 12), label, font=lf, fill=(90, 90, 90))
        d.text((lab_w, y), SAMPLE, font=f, fill=(0, 0, 0))
        d.line((0, y + row_h - 4, w, y + row_h - 4), fill=(230, 230, 230))
    img.save(PREVIEW / "preview_30px.png")
    print("preview_30px.png", img.size)


def render_large(size=48):
    row_h, lab_w, pad = size + 18, 150, 16
    lf = label_font(14)
    w = lab_w + size * len(SAMPLE) + 40
    img = Image.new("RGB", (w, row_h * len(ROWS) + pad * 2), "white")
    d = ImageDraw.Draw(img)
    for r, (label, path) in enumerate(ROWS):
        f = ImageFont.truetype(str(path), size)
        y = pad + r * row_h
        d.text((8, y + size // 2 - 8), label, font=lf, fill=(90, 90, 90))
        d.text((lab_w, y), SAMPLE, font=f, fill=(0, 0, 0))
    img.save(PREVIEW / "preview_48px.png")
    print("preview_48px.png", img.size)


if __name__ == "__main__":
    render_30px()
    render_large()
