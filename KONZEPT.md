# UwULock – Konzept

UwULock ist der Passwortmanager der UwUSuite. Er fängt als Client für einen
Tresor an, den es schon gibt – Vaultwarden oder Bitwarden – und soll später
einen eigenen, selbst gehosteten Server bekommen und mehr können als ein
reiner Passwortmanager.

## Warum erst der Client?

- **Der Tresor ist schon da.** Mein Vaultwarden läuft, die Daten sind drin,
  die Handy-Apps und Browser-Erweiterungen von Bitwarden funktionieren damit.
  Ein Client, der dasselbe Protokoll spricht, ist sofort nützlich und kostet
  keine Migration.
- **Das Protokoll ist offen und gut abgehangen.** Bitwardens Clients und SDK
  sind Open Source, Vaultwarden implementiert dieselbe API. Die Verschlüsselung
  (PBKDF2/Argon2id, HKDF, AES-256-CBC + HMAC, RSA-OAEP) ist dokumentiert und
  lässt sich gegen Bitwardens eigene Testwerte prüfen – das macht UwULock.
- **Der Server kann später kommen, ohne dass der Client weggeworfen wird.**
  Spricht der eigene UwULock-Server dieselbe API, bleibt der Client, wie er
  ist, und die offiziellen Bitwarden-Apps funktionieren weiter.

Die Risiken, offen benannt:

- Bitwardens API ist kein offizieller Vertrag für fremde Clients. Neue
  Funktionen (neue Verschlüsselungsformate, neue Anmeldeprüfungen) muss
  UwULock nachziehen. Vaultwarden ändert sich langsamer und ist das
  Hauptziel.
- Ein Fehler beim **Schreiben** kann echte Daten beschädigen. Darum liest die
  erste Beta nur; Bearbeiten kommt, wenn der Lesepfad in echter Benutzung
  hält, und zuerst gegen den Spielzeug-Server getestet.

## Grundsätze

1. **Das Master-Passwort verlässt das Gerät nie.** Nur der Hash, nur beim
   Anmelden.
2. **Geheimnisse bleiben in Rust.** Die Web-Oberfläche bekommt ein Passwort
   nur, wenn man auf das Auge klickt; Kopieren geht direkt in die
   Zwischenablage und wird wieder geleert.
3. **Offline zuerst.** Die letzte Synchronisation liegt verschlüsselt auf dem
   Gerät und öffnet sich ohne Netz.
4. **Sperren heißt vergessen.** Alles Entschlüsselte fliegt raus – von Hand,
   nach Leerlauf, beim Beenden.
5. **Sicherheit ist nie verspielt.** Nyu darf winken, aber nicht neben einer
   Warnung.

## Kompatibilität mit der UwUSuite

- Gleiches Design-System (Tokens von UwUMail), Nyu mit eigener Hülle
  (Vorhängeschloss), gleiche Titelleiste, Einstellungen und Tonalität.
- Gleicher Installer, gleiches Update-Format (signiert, Kanäle Stabil/Beta,
  Feeds auf dem `updates`-Branch), gleiche Release-Dateinamen für
  uwu.minifyx.de.
- Deutsch als Quellsprache, Englisch als Katalog, geprüft von
  `scripts/check-i18n.mjs`.
- Später: UwUSSH, UwURDP und UwUMail holen Schlüssel und Passwörter aus
  UwULock statt aus eigenen Tresoren (siehe [Roadmap](docs/roadmap.md)).
