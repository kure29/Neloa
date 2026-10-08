# Noto Sans SC (subset)

These are subsets of Noto Sans SC 2.004 (weights 400, 500 and 600), licensed
under the SIL Open Font License 1.1 — see `OFL.txt`. They are bundled so
Chinese text renders the same on macOS, Windows, Linux, iOS and Android.

Each file keeps only CJK code points: the 6,763 hanzi of GB 2312, every CJK
character used in `src/` and `index.html`, CJK symbols and punctuation
(U+3000–U+303F) and full-width forms (U+FF01–U+FF5E). Latin text keeps the
platform's own UI font, and any rarer character falls back to the system CJK
font.

Regenerate after adding UI copy with characters outside GB 2312:

```sh
pip install fonttools brotli
# chars.txt: GB 2312 hanzi + CJK characters found in src/ + U+3000–303F + U+FF01–FF5E
pyftsubset NotoSansSC_400Regular.ttf --text-file=chars.txt --flavor=woff2 \
  --layout-features='*' --no-hinting --desubroutinize \
  --output-file=NotoSansSC-400.woff2   # repeat for 500 and 600
```
