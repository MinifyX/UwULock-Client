// The SHA-256 fingerprints of the certificates an APK is signed with, read from
// its APK Signing Block (signature schemes v2 and v3), so checking the release
// APK needs no Android SDK on the release machine. The same digest as
// `apksigner verify --print-certs` shows as "certificate SHA-256 digest".
//
//   node scripts/apk-cert.mjs <file.apk>

import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const MAGIC = 'APK Sig Block 42';
const SCHEMES = new Set([0x7109871a, 0xf05368c0]); // v2, v3

/** Hex SHA-256 of every signer certificate in the APK; empty when it has no v2/v3 signature. */
export function apkCertificates(bytes) {
  const buf = Buffer.from(bytes);
  // End of central directory: the last "PK\5\6" with room for its 22 bytes.
  let eocd = -1;
  for (let i = buf.length - 22; i >= Math.max(0, buf.length - 22 - 0xffff); i--) {
    if (buf.readUInt32LE(i) === 0x06054b50) {
      eocd = i;
      break;
    }
  }
  if (eocd < 0) throw new Error('Not a ZIP file');
  const central = buf.readUInt32LE(eocd + 16);
  if (central < 32 || buf.toString('latin1', central - 16, central) !== MAGIC) return [];
  const size = Number(buf.readBigUInt64LE(central - 24));
  const start = central - size - 8;
  if (start < 0 || Number(buf.readBigUInt64LE(start)) !== size)
    throw new Error('Broken signing block');

  const found = new Set();
  // A length-prefixed (uint32) sequence of length-prefixed items.
  const items = (from, to) => {
    const out = [];
    for (let at = from; at < to;) {
      const len = buf.readUInt32LE(at);
      out.push([at + 4, at + 4 + len]);
      at += 4 + len;
    }
    return out;
  };
  const sequence = (at) => [at + 4, at + 4 + buf.readUInt32LE(at)];

  for (let at = start + 8; at < central - 24;) {
    const len = Number(buf.readBigUInt64LE(at));
    const id = buf.readUInt32LE(at + 8);
    const value = at + 12;
    if (SCHEMES.has(id)) {
      const [signersFrom, signersTo] = sequence(value);
      for (const [signer] of items(signersFrom, signersTo)) {
        // signer: signed data, … ; signed data: digests, certificates, …
        const [signed] = sequence(signer);
        const [, digestsEnd] = sequence(signed);
        const [certsFrom, certsTo] = sequence(digestsEnd);
        for (const [from, to] of items(certsFrom, certsTo)) {
          found.add(createHash('sha256').update(buf.subarray(from, to)).digest('hex'));
        }
      }
    }
    at += 8 + len;
  }
  return [...found];
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  for (const cert of apkCertificates(readFileSync(process.argv[2]))) console.log(cert);
}
