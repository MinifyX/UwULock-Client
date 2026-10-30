// The update feeds installed apps look for on the `updates` branch, written
// by `pnpm release`.
//
// Each platform gets what it updates itself with: the Windows setup, the bare
// setup program from the macOS disk image (one universal file for both Mac
// platforms), the Linux setup AppImage for copies an earlier setup installed,
// and the .deb / .rpm for copies dpkg or rpm installed (`linux-x86_64-deb`,
// `linux-aarch64-rpm`, …). The URLs point at the release's file names without
// a version; each signature was made on the same bytes under the versioned
// name the installed app checks for. Versions with a suffix (-beta.1) only go
// into the Beta feed, plain versions into Stable and Beta.
//
// The native Mac app (the disk image's Direct build, apps/apple) updates itself
// with Sparkle instead: `appcast-stable.xml` / `appcast-beta.xml` next to the
// JSON feeds, by the same channel rule, with the disk image, its EdDSA
// signature (scripts/sparkle.mjs) and the notes in German and English. The
// macOS entries of the JSON feeds stay for the Tauri app still installed: its
// updater runs the migration setup, which puts the native app in its place.

export const REPOSITORY = 'MinifyX/UwULock-Client';
export const FEED_BRANCH = 'updates';

export const downloadUrl = (version, name) =>
  `https://github.com/${REPOSITORY}/releases/download/v${version}/${name}`;

/** The oldest macOS the native app runs on (apps/apple/project.yml deploymentTarget). */
export const MINIMUM_MACOS = '27.0';

const escapeXml = (text) =>
  text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

/** The notes' Markdown (bullet points, `code`, **bold**) as the little HTML Sparkle shows. */
export function notesHtml(markdown) {
  const inline = (text) =>
    escapeXml(text)
      .replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>')
      .replace(/`(.+?)`/g, '<code>$1</code>');
  const html = [];
  let list = false;
  for (const line of markdown.split('\n').map((l) => l.trim())) {
    const bullet = line.match(/^[-*] (.*)$/);
    if (bullet && !list) html.push('<ul>');
    if (!bullet && list) html.push('</ul>');
    list = Boolean(bullet);
    if (bullet) html.push(`<li>${inline(bullet[1])}</li>`);
    else if (line) html.push(`<p>${inline(line)}</p>`);
  }
  if (list) html.push('</ul>');
  return html.join('');
}

/**
 * Sparkle's appcast with one item, the release. `sparkle` is
 * `{ name, signature, length, build }`: the published disk image, its EdDSA
 * signature and size, and the app's CFBundleVersion (what Sparkle compares).
 */
export function appcast({ version, notes, sparkle, date }) {
  const descriptions = Object.entries(notes)
    .map(
      ([lang, text]) =>
        `      <description xml:lang="${escapeXml(lang)}"><![CDATA[${notesHtml(text).replaceAll(']]>', ']]&gt;')}]]></description>`,
    )
    .join('\n');
  return `<?xml version="1.0" encoding="utf-8"?>
<rss version="2.0" xmlns:sparkle="http://www.andymatuschak.org/xml-namespaces/sparkle" xmlns:dc="http://purl.org/dc/elements/1.1/">
  <channel>
    <title>UwULock</title>
    <link>https://github.com/${REPOSITORY}</link>
    <description>UwULock for macOS</description>
    <item>
      <title>UwULock ${escapeXml(version)}</title>
      <pubDate>${date.toUTCString()}</pubDate>
      <sparkle:version>${escapeXml(String(sparkle.build))}</sparkle:version>
      <sparkle:shortVersionString>${escapeXml(version)}</sparkle:shortVersionString>
      <sparkle:minimumSystemVersion>${MINIMUM_MACOS}</sparkle:minimumSystemVersion>
${descriptions}
      <enclosure url="${escapeXml(downloadUrl(version, sparkle.name))}" length="${Number(sparkle.length)}" type="application/x-apple-diskimage" sparkle:edSignature="${escapeXml(sparkle.signature)}"/>
    </item>
  </channel>
</rss>
`;
}

/**
 * Feed file name → content (an object for the JSON feeds, a string for the appcasts).
 *
 * `setups` maps a Tauri platform key (`windows-x86_64`, `windows-aarch64`,
 * `darwin-aarch64`, `darwin-x86_64`, `linux-x86_64`, `linux-x86_64-deb`, …) to
 * `{ name, signature }`, `name` being the published file. `sparkle`, when
 * given, adds the appcasts (see `appcast`).
 */
export function releaseFeeds({ version, notes, setups, sparkle, date = new Date() }) {
  const channels = version.includes('-') ? ['beta'] : ['stable', 'beta'];
  const platforms = Object.fromEntries(
    Object.entries(setups).map(([platform, setup]) => [
      platform,
      { signature: setup.signature, url: downloadUrl(version, setup.name) },
    ]),
  );
  const feeds = {};
  for (const channel of channels) {
    // Tauri's updater format; the notes stay a JSON string the app reads per language.
    feeds[`${channel}.json`] = {
      version,
      notes: JSON.stringify(notes),
      pub_date: date.toISOString().replace(/\.\d+Z$/, 'Z'),
      platforms,
    };
    if (sparkle) feeds[`appcast-${channel}.xml`] = appcast({ version, notes, sparkle, date });
  }
  return feeds;
}
