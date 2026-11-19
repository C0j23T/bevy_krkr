from pathlib import Path

from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen


def font(family, advances):
    names = {point: f"u{point:04x}" for point in advances}
    order = [".notdef", *names.values()]
    metrics = {".notdef": (500, 50)}
    glyphs = {}
    for name in order:
        pen = TTGlyphPen(None)
        if name != ".notdef":
            point = int(name[1:], 16)
            advance = advances[point]
            metrics[name] = (advance, 50)
            y = 650 if point == 0x301 else 0
            width = max(100, advance - 100)
            pen.moveTo((50, y))
            pen.lineTo((50 + width, y))
            pen.lineTo((50 + width, y + (100 if y else 500)))
            pen.lineTo((50, y + (100 if y else 500)))
            pen.closePath()
        glyphs[name] = pen.glyph()

    builder = FontBuilder(1000, isTTF=True)
    builder.setupGlyphOrder(order)
    builder.setupCharacterMap(names)
    builder.setupGlyf(glyphs)
    builder.setupHorizontalMetrics(metrics)
    builder.setupHorizontalHeader(ascent=800, descent=-200)
    builder.setupNameTable(
        {
            "familyName": family,
            "styleName": "Regular",
            "fullName": f"{family} Regular",
            "psName": family.replace(" ", "") + "-Regular",
        }
    )
    builder.setupOS2(sTypoAscender=800, sTypoDescender=-200, usWinAscent=800, usWinDescent=200)
    builder.setupPost()
    builder.font["head"].created = builder.font["head"].modified = 2082844800
    builder.save(Path(__file__).with_name(f"{family.lower().replace(' ', '-')}.ttf"))


font("Layer Test Latin", {0x41: 600, 0x20: 300})
font("Layer Test Fallback", {0x41: 500, 0x301: 0, 0x5D0: 650, 0x5D1: 700, 0xE123: 750})
