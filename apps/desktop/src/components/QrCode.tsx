import { renderSVG } from 'uqr';

/**
 * A QR code, drawn here with uqr (MIT): the text never leaves the window for it. Dark on white
 * in both themes, so every camera reads it. Shared with the browser extension's popup.
 */
export function QrCode({ text, label }: { text: string; label: string }) {
  return (
    <div
      className="qr"
      role="img"
      aria-label={label}
      data-qr
      dangerouslySetInnerHTML={{
        __html: renderSVG(text, { border: 2, whiteColor: '#ffffff', blackColor: '#1c1420' }),
      }}
    />
  );
}
