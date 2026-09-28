/**
 * An own icon from a picture the person picks: PNG, JPEG, WebP or SVG, cut
 * to a square from its middle and drawn at most 128 pixels wide, as PNG. The
 * page does this in a canvas (an SVG is rasterised here, never sent as SVG);
 * Rust reads the PNG again, seals it and stores it.
 */

const ACCEPTED = ['image/png', 'image/jpeg', 'image/webp', 'image/svg+xml'];
const MAX_INPUT = 8 * 1024 * 1024;
const SIZE = 128;

export const ICON_ACCEPT = ACCEPTED.join(',');

export class IconImageError extends Error {}

function read(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(new IconImageError('unreadable'));
    reader.readAsDataURL(file);
  });
}

function load(url: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.onload = () => resolve(image);
    image.onerror = () => reject(new IconImageError('undecodable'));
    image.src = url;
  });
}

/** The picture as a square PNG `data:` URL of at most 128 × 128 pixels. */
export async function iconFromFile(file: File): Promise<string> {
  if (!ACCEPTED.includes(file.type)) throw new IconImageError('type');
  if (file.size > MAX_INPUT) throw new IconImageError('size');
  const image = await load(await read(file));
  const vector = file.type === 'image/svg+xml';
  // An SVG without a size of its own is drawn at the icon's size.
  const width = image.naturalWidth || SIZE;
  const height = image.naturalHeight || SIZE;
  const side = Math.min(width, height);
  const out = vector ? SIZE : Math.min(SIZE, side);
  const canvas = document.createElement('canvas');
  canvas.width = out;
  canvas.height = out;
  const context = canvas.getContext('2d');
  if (!context) throw new IconImageError('canvas');
  context.imageSmoothingEnabled = true;
  context.imageSmoothingQuality = 'high';
  context.drawImage(image, (width - side) / 2, (height - side) / 2, side, side, 0, 0, out, out);
  return canvas.toDataURL('image/png');
}
