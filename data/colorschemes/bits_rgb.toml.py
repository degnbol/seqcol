#!/usr/bin/env python3
import json
import unicodedata

print('description = "Red, green and blue values from bits 0 to 2, 3 to 5 and 6 to 8 of the code point, '
      'for letters, numbers, punctuation and symbols from U+0021 to U+052F. '
      'Each of the 512 colors occurs once in U+0100 to U+02FF."')
print()
print("[colors]")
for i in range(0x21, 0x530):
    c = chr(i)
    if unicodedata.category(c)[0] in "LNPS":
        r, g, b = (round((i >> shift & 7) * 255 / 7) for shift in (0, 3, 6))
        print(f'{json.dumps(c, ensure_ascii=False)} = "#{r:02x}{g:02x}{b:02x}"')
