# UwU Sans

The default interface font of UwULock, the same file as UwUMail's: [Atkinson
Hyperlegible Next](https://github.com/googlefonts/atkinson-hyperlegible-next)
with its letters untouched, plus Nyu (U+E000), a heart (U+2665), arrows and
`calt` ligatures that turn `:3` into Nyu and `<3` into a heart. Variable,
weight 200-800. License: SIL OFL 1.1 ([OFL.txt](OFL.txt)), changes in
[FONTLOG.txt](FONTLOG.txt).

It is built in UwUMail-Client (`brand/fonts/uwu-sans/build.py`); this repo only
carries the result, byte-identical, at
`apps/desktop/src/assets/fonts/UwUSans[wght].woff2` (the extension takes it
from there too). After a rebuild there, copy it over.

UwULock switches the `:3`/`<3` ligatures off everywhere vault data shows
(see `docs/design.md`): a password or a name must read exactly as stored.
