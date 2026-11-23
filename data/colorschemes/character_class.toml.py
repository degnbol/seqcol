#!/usr/bin/env python3
import json
import unicodedata

# OKLCH lightness 0.70 and chroma 0.12, converted to sRGB. The line comment gives the hue.
COLORS = {
    "Lu": "#df7f78",  # 25
    "Lt": "#df7f78",
    "Ll": "#6cb26f",  # 145
    "Lm": "#6cb26f",
    "Lo": "#6cb26f",
    "N": "#61a3e6",  # 250
    "P": "#c1983a",  # 85
    "S": "#bf85cd",  # 320
}

print('description = "Color by Unicode general category, for U+0021 to U+052F: uppercase and titlecase letters (red), '
      'other letters (green), numbers (blue), punctuation (yellow), symbols (purple)."')
print()
print("[colors]")
for c in map(chr, range(0x21, 0x530)):
    category = unicodedata.category(c)
    color = COLORS.get(category) or COLORS.get(category[0])
    if color:
        print(f"{json.dumps(c, ensure_ascii=False)} = {json.dumps(color)}")
