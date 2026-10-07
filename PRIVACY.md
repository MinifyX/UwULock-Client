# Privacy policy · UwULock browser extension

[Deutsch weiter unten](#datenschutzerklärung--uwulock-browser-erweiterung)

UwULock is a password manager. It talks to exactly one place: **the server you log in to** —
your own UwULock Server, a Vaultwarden, a self-hosted Bitwarden, or bitwarden.com /
bitwarden.eu if you choose them. The extension has no server of its own; neither the developer
nor anybody else gets data from it.

## What goes to your server

- **Your login**: your email address, a hash of your master password (the password itself never
  leaves the browser), two-step login codes, and a device name and id for this browser, so the
  server can list and log out your devices.
- **Your vault**: the items you keep in it — logins, passkeys, notes, cards, identities,
  folders. They are encrypted in the browser before they are sent (end-to-end, the same
  encryption the official Bitwarden apps use); the server stores them but can't read them.
- **Icons** (UwULock Server only, while the setting is on): the host names of your items'
  websites, to get their icons from your server. They are never fetched from the sites
  themselves.
- **A live connection** to your server, so changes from your other devices arrive right away.
- **Masked addresses** (UwULock Server with UwUMail, only when you make one): the domain of the
  page or item you make the address for, in plain text, so the server can label the address.
- **Sends and file requests** (only when you share something): the values you choose from an
  item — with UwULock Server, as an entry Send, optionally including the key of its one-time
  code — go to your server encrypted with a key that only the link holds. **Whoever has the
  link can read them**, and with the one-time code key keep making codes, also after the Send
  is deleted. File requests you look at are listed from your server the same way.

Firefox (at install) and the Chrome Web Store list this as authentication information,
personally identifying information, financial and payment information and website activity
(the domain of a masked address): those are the kinds of data a vault holds, even though most
of it travels encrypted.

## What stays in the browser

The encrypted vault, the session tokens and your settings are kept in the extension's own
storage. While the vault is unlocked, its key is held in memory. Web pages can't read anything
from your vault unless you fill an item into them; they can notice that the extension is
installed (it offers passkeys to every https page and shows its menu next to login fields).

In Safari the extension comes inside the UwULock app but keeps all of this to itself: it
exchanges nothing with the app, offers no passkeys to pages (the app does that, as the system's
passkey provider) and asks Safari for access to the websites you allow.

## What UwULock doesn't do

No analytics, no telemetry, no advertising, no tracking, no crash reports, no third-party
services, no code loaded from anywhere. Nothing is sold or shared.

## Your data

What your server keeps is up to whoever runs it — on your own server, that is you; for
bitwarden.com or bitwarden.eu, Bitwarden's privacy policy applies. Log out to remove the
account from the browser; remove the extension to delete everything it stored.

## Contact

MinifyX — questions and requests as an issue on
[github.com/MinifyX/UwULock-Client](https://github.com/MinifyX/UwULock-Client/issues)

---

# Datenschutzerklärung · UwULock Browser-Erweiterung

UwULock ist ein Passwortmanager. Er spricht mit genau einer Stelle: **dem Server, bei dem du
dich anmeldest** – deinem eigenen UwULock Server, einem Vaultwarden, einem selbst gehosteten
Bitwarden oder bitwarden.com / bitwarden.eu, wenn du die wählst. Die Erweiterung hat keinen
eigenen Server; weder der Entwickler noch sonst jemand bekommt Daten von ihr.

## Was an deinen Server geht

- **Deine Anmeldung**: deine E-Mail-Adresse, ein Hash deines Master-Passworts (das Passwort
  selbst verlässt den Browser nie), Codes der zweistufigen Anmeldung sowie ein Gerätename und
  eine Geräte-ID für diesen Browser, damit der Server deine Geräte anzeigen und abmelden kann.
- **Dein Tresor**: die Einträge darin – Logins, Passkeys, Notizen, Karten, Identitäten, Ordner.
  Sie werden im Browser verschlüsselt, bevor sie gesendet werden (Ende-zu-Ende, dieselbe
  Verschlüsselung wie in den offiziellen Bitwarden-Apps); der Server speichert sie, kann sie aber
  nicht lesen.
- **Icons** (nur bei UwULock Server, solange die Einstellung an ist): die Hostnamen der
  Websites deiner Einträge, um deren Icons von deinem Server zu holen. Von den Websites selbst
  werden sie nie geladen.
- **Eine Live-Verbindung** zu deinem Server, damit Änderungen von deinen anderen Geräten sofort
  ankommen.
- **Maskierte Adressen** (UwULock Server mit UwUMail, nur wenn du eine anlegst): die Domain der
  Seite oder des Eintrags, für die du die Adresse anlegst, im Klartext, damit der Server die
  Adresse beschriften kann.
- **Sends und Dateianfragen** (nur wenn du etwas teilst): die Werte, die du aus einem Eintrag
  wählst – mit UwULock Server als Eintrags-Send, auf Wunsch mit dem Schlüssel des Einmal-Codes –
  gehen verschlüsselt an deinen Server, mit einem Schlüssel, den nur der Link enthält. **Wer den
  Link hat, kann sie lesen**, und mit dem Einmal-Code-Schlüssel weiter Codes erzeugen, auch nach
  dem Löschen des Sends. Deine Dateianfragen holt die Erweiterung zum Ansehen ebenso von deinem
  Server.

Firefox (bei der Installation) und der Chrome Web Store zeigen das als Anmeldeinformationen,
persönlich identifizierende Informationen, Finanz- und Zahlungsinformationen sowie
Website-Aktivität (die Domain einer maskierten Adresse) an: Das sind die Arten von Daten, die ein
Tresor enthält, auch wenn das meiste verschlüsselt übertragen wird.

## Was im Browser bleibt

Der verschlüsselte Tresor, die Sitzungstokens und deine Einstellungen liegen im eigenen Speicher
der Erweiterung. Solange der Tresor entsperrt ist, liegt sein Schlüssel im Arbeitsspeicher.
Webseiten können nichts aus deinem Tresor lesen, außer du füllst einen Eintrag bei ihnen aus;
sie können aber merken, dass die Erweiterung installiert ist (sie bietet jeder https-Seite
Passkeys an und zeigt ihr Menü neben Anmeldefeldern).

In Safari steckt die Erweiterung in der UwULock-App, behält das alles aber für sich: Sie tauscht
nichts mit der App aus, bietet Seiten keine Passkeys an (das macht die App als Passkey-Anbieter
des Systems) und bekommt von Safari nur Zugriff auf die Websites, die du erlaubst.

## Was UwULock nicht tut

Keine Analyse, keine Telemetrie, keine Werbung, kein Tracking, keine Absturzberichte, keine
Dienste Dritter, kein von irgendwo nachgeladener Code. Nichts wird verkauft oder weitergegeben.

## Deine Daten

Was dein Server speichert, bestimmt, wer ihn betreibt – bei deinem eigenen Server also du; für
bitwarden.com oder bitwarden.eu gilt die Datenschutzerklärung von Bitwarden. Melde dich ab, um
das Konto aus dem Browser zu entfernen; entferne die Erweiterung, um alles zu löschen, was sie
gespeichert hat.

## Kontakt

MinifyX – Fragen und Anliegen als Issue auf
[github.com/MinifyX/UwULock-Client](https://github.com/MinifyX/UwULock-Client/issues)
