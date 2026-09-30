# Sample font

`body.ttf` is an ASCII-only subset of Roboto Mono Regular, sourced from
https://github.com/googlefonts/RobotoMono/blob/main/fonts/ttf/RobotoMono-Regular.ttf
under the adjacent SIL Open Font License. Downloaded 2026-09-28.

Generated with fonttools `pyftsubset --unicodes=U+0020-007E
--no-layout-closure --drop-tables+=GSUB,GPOS,kern`, then decomposed the colon and
semicolon compound outlines with `DecomposingRecordingPen` and `TTGlyphPen`.
No system font lookup is needed. This is a test/example resource, not a new
default game font.
