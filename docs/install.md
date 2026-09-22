# Installing UwULock

[Deutsch weiter unten](#uwulock-installieren)

UwULock runs on **Windows 10 and 11** (x64 and ARM), **macOS 11 or newer**
(Apple silicon and Intel) and **Linux** (x86_64 and arm64). On Windows and
macOS the setup installs for your user only — no admin rights. UwULock speaks
English or German, following the system; **Settings → Appearance → Language**
switches.

It is a first beta. I use it every day on Windows; the macOS and Linux builds
come out of CI and haven't been tried by hand yet, so expect more rough edges
there.

Windows and macOS get UwULock's own setup with Nyu in it, Linux a package for
your distribution or a portable folder. Download from the
[releases](https://github.com/MinifyX/UwULock-Client/releases): take the newest
one at the top. Betas are marked **Pre-release**; the newest version without
that mark is the stable one. The file names carry no version, so
`https://github.com/MinifyX/UwULock-Client/releases/latest/download/<file>`
always gets the newest stable one.

| System                     | File under **Assets**                                                |
| -------------------------- | -------------------------------------------------------------------- |
| Windows 10/11 (x64)        | `UwULock-windows-x64-setup.exe`                                      |
| Windows 11 on ARM          | `UwULock-windows-arm64-setup.exe`                                    |
| macOS (Intel & Apple chip) | `UwULock-macos-universal.dmg`                                        |
| Ubuntu / Debian            | `UwULock-linux-x64.deb` · ARM: `UwULock-linux-arm64.deb`             |
| Fedora / openSUSE          | `UwULock-linux-x64.rpm` · ARM: `UwULock-linux-arm64.rpm`             |
| Linux, portable            | `UwULock-linux-x64-portable.tar.gz` · ARM: `…-arm64-portable.tar.gz` |
| Arch Linux                 | planned: an AUR package `uwulock-bin`; until then the portable one   |

The `UwULock-update-…` files next to them are for the in-app updater; you
don't need them.

**Checking the download (optional).** Each release has a `SHA256SUMS.txt`. On
macOS and Linux: `shasum -a 256 -c SHA256SUMS.txt --ignore-missing` in the
download folder. On Windows, in PowerShell:
`Get-FileHash "$env:USERPROFILE\Downloads\UwULock-windows-x64-setup.exe"` and
compare with the line in the file.

## Windows

Double-click the setup. Windows will most likely show **"Windows protected your
PC"**: the setup isn't signed with a paid code-signing certificate, so
SmartScreen doesn't know it yet. Click **More info**, then **Run anyway**. Your
browser may also say the file is "not commonly downloaded"; keep it anyway (in
Edge: `…` → **Keep** → **Show more** → **Keep anyway**).

- **Install** sets everything up in a few seconds.
- **Options** lets you change the folder (default
  `%LOCALAPPDATA%\Programs\UwULock`) or turn off the desktop shortcut.
- If Microsoft Edge WebView2 is missing (Windows 11 always has it), the setup
  offers to download and install it.

Uninstall from **Windows Settings → Apps → Installed apps → UwULock**.

## macOS

Open the `.dmg` and double-click **UwULock Setup**. UwULock isn't notarized by
Apple (that needs a paid developer account), so the first time macOS says it
can't check the app. Then:

1. Open **System Settings → Privacy & Security**.
2. Scroll down: next to "UwULock Setup was blocked", click **Open Anyway** and
   confirm.

(On macOS 14 and older, right-clicking the setup and choosing **Open** works
too.) The setup installs **UwULock** into `/Applications`, or into
`~/Applications` if your user may not write to `/Applications`. The installed
app starts without that question.

To uninstall, run the setup again and choose **Uninstall …** — it asks whether
to keep your login and settings. Dragging the app to the Trash works too, but
leaves the data in `~/Library/Application Support/app.uwulock.desktop`.

## Linux

**Ubuntu, Debian and relatives:** `sudo apt install ./UwULock-linux-x64.deb`
(`…-arm64.deb` on ARM). **Fedora, openSUSE:**
`sudo dnf install ./UwULock-linux-x64.rpm` or
`sudo zypper install ./UwULock-linux-x64.rpm`. Both install the app
system-wide as package `uwulock`, with a menu entry, using the system's
WebKitGTK 4.1, and update themselves: UwULock
downloads the next package and installs it on **Restart now**, asking for the
administrator password. Uninstall with `sudo apt remove uwulock` or
`sudo dnf remove uwulock`.

**Portable:** unpack `UwULock-linux-x64-portable.tar.gz` anywhere and start
`./UwULock/uwulock`. It brings its own WebKit, installs nothing and doesn't
update itself — fetch the newest one to update.

**Arch Linux:** an AUR package `uwulock-bin` is planned. Until it exists, the
portable folder works.

## First steps

- **Log in** with the server your vault is on: **Self-hosted** for a
  Vaultwarden (or a self-hosted Bitwarden) — the address you open the web
  vault at, like `https://vault.example.org` — or **bitwarden.com** /
  **bitwarden.eu** for Bitwarden's cloud. Then your email and master password.
- **Two-step login** works with an authenticator app, email codes and YubiKey
  OTP. Duo and passkeys/FIDO2 as the only second step don't work yet: add an
  authenticator app in the web vault as well.
- The master password never leaves your computer: UwULock derives the key from
  it, the same way Bitwarden's apps do, and sends the server only a hash.
- After that, UwULock keeps an **encrypted copy** of the vault on this device
  and opens it with the master password — offline too. It syncs on unlock and
  every five minutes.
- **Ctrl+F** searches, **Ctrl+U / Ctrl+P / Ctrl+T** copy the username,
  password and one-time code of the selected login, **Ctrl+L** locks,
  **Ctrl+G** opens the password generator. Copied values leave the clipboard
  again after 30 seconds (Settings → Security), and on Windows they never show
  up in the clipboard history.
- The vault **locks by itself** after 15 minutes without input, and always
  when UwULock quits.
- **This beta only reads.** Creating and editing items, attachments and sends
  are still done in the web vault; UwULock picks the changes up with the next
  sync.

## Updates

UwULock updates itself: about 20 seconds after it starts, and every six hours,
it looks for a newer version, downloads it quietly (signed and checked) and
offers a restart. **Settings → Updates** switches between the Beta and Stable
channels. Stable only gets versions without a beta mark; as long as there are
only betas, stay on Beta.

A newer setup can also simply be run over an installed UwULock, a newer package
installed over the old one. The login and settings stay. The portable
folder doesn't update itself.

## Where your data lives

| What                                          | Windows                               | macOS                                               | Linux                                |
| --------------------------------------------- | ------------------------------------- | --------------------------------------------------- | ------------------------------------ |
| Login, encrypted copy of the vault, device id | `%APPDATA%\app.uwulock.desktop\`      | `~/Library/Application Support/app.uwulock.desktop` | `~/.local/share/app.uwulock.desktop` |
| App settings (look, security, …)              | `%LOCALAPPDATA%\app.uwulock.desktop\` | `~/Library/WebKit/app.uwulock.desktop`              | `~/.local/share/app.uwulock.desktop` |
| The program                                   | `%LOCALAPPDATA%\Programs\UwULock\`    | `/Applications/UwULock.app`                         | `/usr/bin/uwulock-desktop`           |

`account.json` holds the server, your email and the keys as the server wraps
them — useless without the master password. `vault.json` is the last sync,
exactly as the server sent it: every name, username, password and note in it
is still encrypted by Bitwarden. **Log out** (Settings → Account) removes both.

## If something goes wrong

- **"WebView2 couldn't be installed"** (Windows): install the Evergreen WebView2
  Runtime from [Microsoft](https://developer.microsoft.com/microsoft-edge/webview2/)
  and run the setup again.
- **An antivirus program blocks the setup**: that is the same missing
  certificate as SmartScreen's warning. The source of every release is in this
  repository, and the checksums tell you the file is the published one.
- **macOS says the app is damaged**: that happens when the quarantine mark
  survives on the installed app, which the setup avoids. Running
  `xattr -dr com.apple.quarantine /Applications/UwULock.app` clears it.
- **Windows on ARM** has its own setup (`UwULock-windows-arm64-setup.exe`); the
  x64 one runs there too, emulated and slower.
- **A package update fails** (no `pkexec`, or the password prompt was
  cancelled): download the newest `.deb` / `.rpm` and install it as above.
- **"The server must be reachable over https://"**: UwULock only talks to a
  server over TLS (except on `localhost`). Put your Vaultwarden behind a
  reverse proxy with a certificate — which the Bitwarden apps need anyway.
- **A certificate error with a homelab CA**: UwULock trusts the certificates
  your system trusts. Import your CA into the system's store.
- **"Email address or master password is wrong"** though both are right: the
  address has to be the server's own, not a path inside the web vault. Try the
  address without anything after the host name.
- **The login asks for a code from an email** (bitwarden.com / .eu): Bitwarden
  checks new devices; the code is in your inbox.
- Something else? [Open an issue](https://github.com/MinifyX/UwULock-Client/issues)
  — no promises on how fast, see the README.

Building it yourself instead: [Development](../README.md#development).

---

# UwULock installieren

UwULock läuft unter **Windows 10 und 11** (x64 und ARM), **macOS 11 oder
neuer** (Apple-Chip und Intel) und **Linux** (x86_64 und arm64). Unter Windows
und macOS installiert das Setup nur für deinen Benutzer — ohne Adminrechte.
UwULock spricht Deutsch oder Englisch, je nach System; **Einstellungen →
Darstellung → Sprache** schaltet um.

Es ist eine erste Beta. Ich nutze sie jeden Tag unter Windows; die Versionen
für macOS und Linux baut die CI, von Hand ausprobiert hat sie noch niemand —
dort also mit mehr Ecken und Kanten rechnen.

Windows und macOS bekommen UwULocks eigenes Setup mit Nyu, Linux ein Paket für
deine Distribution oder einen portablen Ordner. Lade von den
[Releases](https://github.com/MinifyX/UwULock-Client/releases) herunter: das
neueste ganz oben. Betas sind als **Pre-release** markiert; die neueste
Version ohne diese Markierung ist die stabile. Die Dateinamen enthalten keine
Version, `https://github.com/MinifyX/UwULock-Client/releases/latest/download/<Datei>`
holt also immer die neueste stabile.

| System                     | Datei unter **Assets**                                               |
| -------------------------- | -------------------------------------------------------------------- |
| Windows 10/11 (x64)        | `UwULock-windows-x64-setup.exe`                                      |
| Windows 11 auf ARM         | `UwULock-windows-arm64-setup.exe`                                    |
| macOS (Intel & Apple-Chip) | `UwULock-macos-universal.dmg`                                        |
| Ubuntu / Debian            | `UwULock-linux-x64.deb` · ARM: `UwULock-linux-arm64.deb`             |
| Fedora / openSUSE          | `UwULock-linux-x64.rpm` · ARM: `UwULock-linux-arm64.rpm`             |
| Linux, portabel            | `UwULock-linux-x64-portable.tar.gz` · ARM: `…-arm64-portable.tar.gz` |
| Arch Linux                 | geplant: ein AUR-Paket `uwulock-bin`; bis dahin die portable Version |

Die `UwULock-update-…`-Dateien daneben sind für den Updater in der App; du
brauchst sie nicht.

**Download prüfen (optional).** Jedes Release hat eine `SHA256SUMS.txt`. Unter
macOS und Linux im Download-Ordner: `shasum -a 256 -c SHA256SUMS.txt
--ignore-missing`. Unter Windows in PowerShell:
`Get-FileHash "$env:USERPROFILE\Downloads\UwULock-windows-x64-setup.exe"` und
mit der Zeile in der Datei vergleichen.

## Windows

Doppelklick auf das Setup. Windows zeigt sehr wahrscheinlich **„Der Computer
wurde durch Windows geschützt“**: Das Setup ist nicht mit einem
kostenpflichtigen Code-Signing-Zertifikat signiert, deshalb kennt SmartScreen es
noch nicht. Klick auf **Weitere Informationen**, dann auf **Trotzdem
ausführen**. Der Browser meldet vielleicht, die Datei werde „nicht häufig
heruntergeladen“; behalte sie trotzdem (in Edge: `…` → **Beibehalten** → **Mehr
anzeigen** → **Trotzdem beibehalten**).

- **Installieren** richtet alles in ein paar Sekunden ein.
- Unter **Optionen** änderst du den Ordner (Standard
  `%LOCALAPPDATA%\Programs\UwULock`) oder schaltest die Desktop-Verknüpfung ab.
- Fehlt Microsoft Edge WebView2 (Windows 11 hat es immer), bietet das Setup an,
  es herunterzuladen und zu installieren.

Deinstallieren über **Windows-Einstellungen → Apps → Installierte Apps →
UwULock**.

## macOS

Die `.dmg` öffnen und **UwULock Setup** doppelklicken. UwULock ist nicht bei
Apple notarisiert (das braucht einen kostenpflichtigen Entwickler-Account),
deshalb sagt macOS beim ersten Mal, es könne die App nicht prüfen. Dann:

1. **Systemeinstellungen → Datenschutz & Sicherheit** öffnen.
2. Nach unten scrollen: neben „UwULock Setup wurde blockiert“ auf **Trotzdem
   öffnen** klicken und bestätigen.

(Unter macOS 14 und älter geht auch Rechtsklick auf das Setup → **Öffnen**.) Das
Setup installiert **UwULock** nach `/Applications`, oder nach `~/Applications`,
wenn dein Benutzer nicht in `/Applications` schreiben darf. Die installierte
App startet ohne diese Rückfrage.

Deinstallieren: das Setup noch einmal starten und **Deinstallieren …** wählen —
es fragt, ob Anmeldung und Einstellungen bleiben sollen. Die App in den Papierkorb ziehen
geht auch, lässt aber die Daten in
`~/Library/Application Support/app.uwulock.desktop` liegen.

## Linux

**Ubuntu, Debian und Verwandte:** `sudo apt install ./UwULock-linux-x64.deb`
(`…-arm64.deb` auf ARM). **Fedora, openSUSE:**
`sudo dnf install ./UwULock-linux-x64.rpm` oder
`sudo zypper install ./UwULock-linux-x64.rpm`. Beide installieren die App
systemweit als Paket `uwulock`, mit Eintrag im Anwendungsmenü, nutzen das
WebKitGTK 4.1 des Systems und aktualisieren sich selbst: UwULock lädt das nächste Paket und installiert es bei
**Jetzt neu starten**, nach Eingabe des Administrator-Passworts.
Deinstallieren mit `sudo apt remove uwulock` bzw. `sudo dnf remove uwulock`.

**Portabel:** `UwULock-linux-x64-portable.tar.gz` irgendwo entpacken und
`./UwULock/uwulock` starten. Bringt sein eigenes WebKit mit, installiert nichts
und aktualisiert sich nicht — zum Aktualisieren die neueste holen.

**Arch Linux:** Ein AUR-Paket `uwulock-bin` ist geplant. Bis es das gibt, geht
die portable Version.

## Erste Schritte

- **Anmelden** beim Server, auf dem dein Tresor liegt: **Selbst gehostet** für
  einen Vaultwarden (oder ein selbst gehostetes Bitwarden) — die Adresse, unter
  der du den Web-Tresor öffnest, etwa `https://vault.example.org` — oder
  **bitwarden.com** / **bitwarden.eu** für Bitwardens Cloud. Dann E-Mail und
  Master-Passwort.
- **Zweistufige Anmeldung** geht mit Authenticator-App, E-Mail-Codes und
  YubiKey OTP. Duo und Passkeys/FIDO2 als einzige zweite Stufe gehen noch
  nicht: Richte im Web-Tresor zusätzlich eine Authenticator-App ein.
- Das Master-Passwort verlässt deinen Rechner nie: UwULock leitet daraus den
  Schlüssel ab, genau wie Bitwardens Apps, und schickt dem Server nur einen
  Hash.
- Danach hält UwULock eine **verschlüsselte Kopie** des Tresors auf diesem
  Gerät und öffnet sie mit dem Master-Passwort — auch offline. Synchronisiert
  wird beim Entsperren und alle fünf Minuten.
- **Strg+F** sucht, **Strg+U / Strg+P / Strg+T** kopieren Benutzername,
  Passwort und Einmal-Code des gewählten Logins, **Strg+L** sperrt, **Strg+G**
  öffnet den Passwort-Generator. Kopiertes verschwindet nach 30 Sekunden
  wieder aus der Zwischenablage (Einstellungen → Sicherheit) und landet unter
  Windows nie im Zwischenablage-Verlauf.
- Der Tresor **sperrt sich selbst** nach 15 Minuten ohne Eingabe, und immer,
  wenn UwULock beendet wird.
- **Diese Beta liest nur.** Einträge anlegen und bearbeiten, Anhänge und Sends
  gehen noch im Web-Tresor; UwULock holt die Änderungen mit dem nächsten Sync.

## Updates

UwULock aktualisiert sich selbst: etwa 20 Sekunden nach dem Start und danach
alle sechs Stunden sucht es nach einer neuen Version, lädt sie still herunter
(signiert und geprüft) und bietet einen Neustart an. **Einstellungen → Updates**
wechselt zwischen den Kanälen Beta und Stabil. Stabil bekommt nur Versionen
ohne Beta-Markierung; solange es nur Betas gibt, bleib bei Beta.

Ein neueres Setup kann auch einfach über ein installiertes UwULock laufen, ein
neueres Paket über das alte installiert werden. Anmeldung und Einstellungen
bleiben. Der portable Ordner aktualisiert sich nicht selbst.

## Wo deine Daten liegen

| Was                                                    | Windows                               | macOS                                               | Linux                                |
| ------------------------------------------------------ | ------------------------------------- | --------------------------------------------------- | ------------------------------------ |
| Anmeldung, verschlüsselte Kopie des Tresors, Geräte-ID | `%APPDATA%\app.uwulock.desktop\`      | `~/Library/Application Support/app.uwulock.desktop` | `~/.local/share/app.uwulock.desktop` |
| App-Einstellungen (Aussehen, Sicherheit, …)            | `%LOCALAPPDATA%\app.uwulock.desktop\` | `~/Library/WebKit/app.uwulock.desktop`              | `~/.local/share/app.uwulock.desktop` |
| Das Programm                                           | `%LOCALAPPDATA%\Programs\UwULock\`    | `/Applications/UwULock.app`                         | `/usr/bin/uwulock-desktop`           |

`account.json` enthält Server, E-Mail und die Schlüssel so, wie der Server sie
verpackt — ohne Master-Passwort nutzlos. `vault.json` ist der letzte Sync,
genau wie der Server ihn geschickt hat: Jeder Name, Benutzername, jedes
Passwort und jede Notiz darin ist noch von Bitwarden verschlüsselt.
**Abmelden** (Einstellungen → Konto) entfernt beides.

## Wenn etwas nicht klappt

- **„WebView2 couldn't be installed“** (Windows): Installiere die Evergreen
  WebView2 Runtime von
  [Microsoft](https://developer.microsoft.com/microsoft-edge/webview2/) und
  starte das Setup noch einmal.
- **Ein Virenscanner blockiert das Setup**: Das ist dasselbe fehlende Zertifikat
  wie bei SmartScreen. Der Quellcode jedes Releases liegt in diesem Repository,
  und die Prüfsummen zeigen dir, dass die Datei die veröffentlichte ist.
- **macOS sagt, die App sei beschädigt**: Das passiert, wenn die
  Quarantäne-Markierung an der installierten App hängen bleibt, was das Setup
  vermeidet. `xattr -dr com.apple.quarantine /Applications/UwULock.app` entfernt
  sie.
- **Windows auf ARM** hat ein eigenes Setup
  (`UwULock-windows-arm64-setup.exe`); das x64-Setup läuft dort auch, emuliert
  und langsamer.
- **Ein Paket-Update klappt nicht** (kein `pkexec`, oder die Passwortabfrage
  abgebrochen): die neueste `.deb` / `.rpm` herunterladen und wie oben
  installieren.
- **„Der Server muss per https:// erreichbar sein“**: UwULock spricht mit
  einem Server nur über TLS (außer auf `localhost`). Stell deinen Vaultwarden
  hinter einen Reverse Proxy mit Zertifikat — das brauchen die Bitwarden-Apps
  sowieso.
- **Zertifikatsfehler mit einer eigenen Homelab-CA**: UwULock vertraut den
  Zertifikaten, denen dein System vertraut. Importiere deine CA in den
  Zertifikatsspeicher des Systems.
- **„E-Mail-Adresse oder Master-Passwort stimmt nicht“**, obwohl beides stimmt:
  Die Adresse muss die des Servers sein, kein Pfad im Web-Tresor. Probier die
  Adresse ohne alles hinter dem Hostnamen.
- **Die Anmeldung fragt nach einem Code aus einer E-Mail** (bitwarden.com /
  .eu): Bitwarden prüft neue Geräte; der Code liegt in deinem Postfach.
- Etwas anderes? [Issue aufmachen](https://github.com/MinifyX/UwULock-Client/issues)
  — ohne Versprechen, wie schnell, siehe README.
