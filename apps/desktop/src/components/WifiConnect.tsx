import { Button, Icon, ICONS } from '@uwusuite/design';
import { useState } from 'react';
import { copyField, wifiConnect, wifiSettings, type WifiJoined } from '../lib/api';
import { toastError } from '../lib/errors';
import { copiedText } from '../lib/format';
import { t, useLanguage } from '../lib/i18n';
import { getSettings } from '../lib/settings';
import { toast } from '../lib/toast';
import type { WifiView } from '../lib/wifi';

/** Why Android can't take a network (`reason` of an `unsupported` answer, docs/wifi.md). */
function reasonText(reason: string | null): string {
  switch (reason) {
    case 'wep':
      return t(
        'Android lässt Apps keine WEP-Netze mehr hinzufügen. Füge es in den WLAN-Einstellungen selbst hinzu – und stell den Router besser auf WPA2 oder WPA3 um, WEP ist unsicher.',
      );
    case 'eap-tls':
      return t(
        'EAP-TLS braucht ein Client-Zertifikat, das UwULock nicht hat. Richte das WLAN mit dem Zertifikat in den WLAN-Einstellungen ein.',
      );
    case 'eap':
      return t(
        'UwULock übergibt Enterprise-WLANs mit PEAP, TTLS oder PWD. Wähle die EAP-Methode im Eintrag.',
      );
    case 'ca-domain':
      return t(
        'Android prüft bei PEAP und TTLS den Server. UwULock kann das nur über eine Domain im Feld CA-Zertifikat (etwa radius.example.org) und die Zertifikate des Systems. Trag die Domain ein oder richte das WLAN in den WLAN-Einstellungen ein.',
      );
    case 'identity':
      return t('Für ein Enterprise-WLAN fehlt die Identität im Eintrag.');
    case 'no-password':
      return t('Im Eintrag fehlt das Passwort für dieses WLAN.');
    case 'password':
      return t(
        'Android nimmt nur WPA-Passwörter mit 8 bis 63 einfachen Zeichen (ASCII, keine Umlaute). Füge das WLAN in den WLAN-Einstellungen selbst hinzu.',
      );
    case 'no-ssid':
      return t('Ohne Netzwerknamen (SSID) gibt es nichts zu verbinden. Trag ihn im Eintrag ein.');
    default:
      return t('Dieses Gerät kann WLANs nicht aus einer App übernehmen.');
  }
}

/**
 * Android's *Connect* for a Wi-Fi item: the system's own sheet (Android 11+) or a network
 * suggestion (Android 10), and where neither can take the network, an explanation and the way
 * through the Wi-Fi settings with the password copied. Only shown on Android: iOS has no way
 * for a sideloaded app (docs/mobile.md).
 */
export function WifiConnect({ id, wifi }: { id: string; wifi: WifiView }) {
  useLanguage();
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<WifiJoined | null>(null);
  const ssid = wifi.ssid;
  const hasPassword = !!wifi.password?.hasValue && wifi.security !== 'None';

  async function connect() {
    setBusy(true);
    setProblem(null);
    try {
      const joined = await wifiConnect(id);
      switch (joined.outcome) {
        case 'saved':
          toast(
            t(
              '„{ssid}“ ist gespeichert. Das Handy verbindet sich, sobald das WLAN in Reichweite ist.',
              {
                ssid,
              },
            ),
          );
          break;
        case 'already-saved':
          toast(t('„{ssid}“ ist auf diesem Handy schon gespeichert.', { ssid }));
          break;
        case 'suggested':
          toast(
            t(
              'UwULock schlägt „{ssid}“ vor. Erlaube es in Androids Benachrichtigung, dann verbindet sich das Handy, sobald das WLAN in Reichweite ist.',
              { ssid },
            ),
          );
          break;
        case 'declined':
          toast(t('Nicht hinzugefügt – du hast es abgelehnt.'));
          break;
        default:
          setProblem(joined);
      }
    } catch (error) {
      toastError(error);
    } finally {
      setBusy(false);
    }
  }

  async function openSettings() {
    try {
      if (hasPassword && wifi.password) {
        await copyField(id, `field:${wifi.password.index}`);
        toast(copiedText('password', getSettings().clipboardClear));
      }
      await wifiSettings();
    } catch (error) {
      toastError(error);
    }
  }

  const explanation =
    problem?.outcome === 'unsupported'
      ? reasonText(problem.reason)
      : problem?.outcome === 'disallowed'
        ? t(
            'Android lässt UwULock keine WLANs vorschlagen. Erlaube es unter Einstellungen → Apps → Spezieller App-Zugriff → WLAN-Steuerung, oder füge das WLAN selbst hinzu.',
          )
        : problem
          ? t('Android hat das WLAN nicht übernommen: {reason}', {
              reason: problem.message ?? '—',
            })
          : null;

  return (
    <>
      <Button
        variant="primary"
        size="sm"
        icon={ICONS.wifi}
        onClick={connect}
        disabled={busy || !ssid}
        aria-busy={busy || undefined}
        data-wifi-connect
      >
        {t('Verbinden')}
      </Button>
      {explanation && (
        <div className="wifi-connect-problem" role="status">
          <p>{explanation}</p>
          {problem?.reason !== 'no-ssid' && (
            <Button variant="ghost" size="sm" onClick={openSettings}>
              {hasPassword
                ? t('Passwort kopieren & WLAN-Einstellungen öffnen')
                : t('WLAN-Einstellungen öffnen')}
              <Icon icon={ICONS.openExternal} size="xs" />
            </Button>
          )}
        </div>
      )}
    </>
  );
}
