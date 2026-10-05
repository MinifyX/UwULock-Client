# UwU Sans

The default interface font of UwULock, the same file as UwUMail's: [Atkinson
Hyperlegible Next](https://github.com/googlefonts/atkinson-hyperlegible-next)
with its letters untouched, plus Nyu (U+E000), a heart (U+2665) and arrows,
and no ligatures: `:3` and `<3` stay as typed. Variable, weight 200-800. License: SIL OFL 1.1 ([OFL.txt](OFL.txt)), changes in
[FONTLOG.txt](FONTLOG.txt).

It is built in UwUSuite-Design (`fonts/uwu-sans-source/build.py`); this repo only
carries the result, byte-identical, at
`apps/desktop/src/assets/fonts/UwUSans[wght].woff2` (the extension takes it
from there too). After a rebuild there, copy it over.

Until font version 1.100 UwU Sans turned `:3` into Nyu and `<3` into a heart.
That changed the meaning of the symbols and is gone. UwULock keeps contextual
alternates off everywhere (see `docs/design.md`) for JetBrains Mono's sake: a
password or a name must read exactly as stored.
