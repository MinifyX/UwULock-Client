# UwU Sans

The default interface font of UwULock, the same file as UwUMail's: [Atkinson
Hyperlegible Next](https://github.com/googlefonts/atkinson-hyperlegible-next)
with its letters untouched, plus Nyu (U+E000), a heart (U+2665) and arrows,
and no ligatures: `:3` and `<3` stay as typed. Variable, weight 200-800. License: SIL OFL 1.1 ([OFL.txt](OFL.txt)), changes in
[FONTLOG.txt](FONTLOG.txt).

It is built in UwUSuite-Design (`fonts/uwu-sans-source/build.py`), and the
desktop app takes it from the package (`@uwusuite/design/fonts.css`). The
browser extension still carries a byte-identical copy at
`apps/extension/src/legacy/fonts/UwUSans[wght].woff2` until it moves to the
package too.

Until font version 1.100 UwU Sans turned `:3` into Nyu and `<3` into a heart.
That changed the meaning of the symbols and is gone. Values in JetBrains Mono
keep its ligatures off (the package's base styles): a password or a name must
read exactly as stored.
