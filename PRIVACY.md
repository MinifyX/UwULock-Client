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

Firefox lists this at install as authentication information, personally identifying
information and financial and payment information: those are the kinds of data a vault holds,
even though they travel encrypted.

## What stays in the browser

The encrypted vault, the session tokens and your settings are kept in the extension's own
storage. While the vault is unlocked, its key is held in memory. Web pages see nothing unless
you fill an item into them.

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

Firefox zeigt das bei der Installation als Anmeldeinformationen, persönlich identifizierende
Informationen sowie Finanz- und Zahlungsinformationen an: Das sind die Arten von Daten, die ein
Tresor enthält, auch wenn sie verschlüsselt übertragen werden.

## Was im Browser bleibt

Der verschlüsselte Tresor, die Sitzungstokens und deine Einstellungen liegen im eigenen Speicher
der Erweiterung. Solange der Tresor entsperrt ist, liegt sein Schlüssel im Arbeitsspeicher.
Webseiten sehen nichts, außer du füllst einen Eintrag bei ihnen aus.

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
