#!/usr/bin/env bash
# Builds UwULock for the simulator and/or for the iPhone, and packs the iPhone
# app into an .ipa.
#
# Nothing here is signed: UwULock has no Apple developer account, and Xcode
# refuses to build for a real iPhone without one ("requires a development
# team"). So signing is switched off in the generated project before the build.
# A sideloading tool signs the .ipa with your own Apple ID on the way to the
# phone, see docs/mobile.md.
#
# Both builds go through Tauri. They have to: the "Build Rust Code" phase in the
# Xcode project asks the surrounding `tauri ios build` process for its options
# over a local socket, and without it the phase dies with "Abort trap: 6".
# `--no-sign` makes Tauri archive straight away: without it, it first builds
# the app once more on its own (`xcodebuild build`), which costs a whole second
# release build of the Rust code and gives nothing an unsigned app needs.
#
# Usage: scripts/ios-build.sh <version> [output folder] [simulator|iphone|both]
set -euo pipefail

version="$1"
out="${2:-out}"
which="${3:-both}"
gen="apps/desktop/src-tauri/gen/apple"
case "$which" in simulator | iphone | both) ;; *) echo "::error::Unknown build: $which"; exit 2 ;; esac

echo "--- tools ---"
command -v pnpm node cargo rustup xcodegen
rustup target list --installed | grep apple-ios || true

# `tauri ios build` folds Info.ios.plist into the app it exports, which is a
# step we never reach; the generated project keeps its own copy, so the keys go
# in here. Runs again after xcodegen, which rewrites that file.
merge_info_plist() {
  local extra="apps/desktop/src-tauri/Info.ios.plist" generated
  generated=$(find "$gen" -maxdepth 2 -name Info.plist | head -n 1)
  if [ -f "$extra" ] && [ -n "$generated" ]; then
    /usr/libexec/PlistBuddy -c "Merge $extra" "$generated"
    echo "Merged $extra into $generated"
  fi
}

# Signing off, and a copy of what the Rust build phase prints — Xcode throws
# that away, and it is the only place that says why a build died.
node -e '
  const fs = require("fs");
  const file = process.argv[1];
  const text = fs.readFileSync(file, "utf8");
  let patched = text.replace(
    "        ENABLE_BITCODE: false",
    [
      "        ENABLE_BITCODE: false",
      "        CODE_SIGNING_ALLOWED: NO",
      "        CODE_SIGNING_REQUIRED: NO",
      "        CODE_SIGN_IDENTITY: \"\"",
      "        CODE_SIGN_ENTITLEMENTS: \"\"",
    ].join("\n"),
  );
  if (patched === text) console.error("::warning::Could not switch signing off in project.yml");
  const before = patched;
  patched = patched.replace(
    /(- script: )(pnpm tauri ios xcode-script[^\n]*)/,
    (_, head, command) =>
      `${head}${command} > "$SRCROOT/rust-build.log" 2>&1; status=$?; cat "$SRCROOT/rust-build.log"; exit $status`,
  );
  if (patched === before) console.error("::warning::Could not find the Rust build phase in project.yml");
  fs.writeFileSync(file, patched);
' "$gen/project.yml"
# The AutoFill extension for passkeys (apps/desktop/src-tauri/apple/PasskeyProvider, iOS 17+):
# its own target, built into the app's PlugIns. Unsigned like the app; it needs a developer team's
# App Group and Keychain group to reach the vault (docs/passkeys.md).
node -e '
  const fs = require("fs");
  const [gen, short, build] = process.argv.slice(1);
  const lines = [
    "targets:",
    "  UwULockPasskeys:",
    "    type: app-extension",
    "    platform: iOS",
    "    deploymentTarget: \"17.0\"",
    "    sources:",
    "      - path: ../../apple/PasskeyProvider",
    "        excludes: [\"*.entitlements\", \"Info.plist\"]",
    "    settings:",
    "      base:",
    "        PRODUCT_NAME: UwULockPasskeys",
    "        PRODUCT_MODULE_NAME: UwULockPasskeys",
    "        PRODUCT_BUNDLE_IDENTIFIER: app.uwulock.passkeys",
    "        INFOPLIST_FILE: ../../apple/PasskeyProvider/Info.plist",
    "        UWULOCK_APP_GROUP: group.app.uwulock",
    `        MARKETING_VERSION: "${short}"`,
    `        CURRENT_PROJECT_VERSION: "${build}"`,
    "        SWIFT_VERSION: \"5.0\"",
    "        TARGETED_DEVICE_FAMILY: \"1,2\"",
    "        APPLICATION_EXTENSION_API_ONLY: YES",
    "        CODE_SIGNING_ALLOWED: NO",
    "        CODE_SIGNING_REQUIRED: NO",
    "        CODE_SIGN_IDENTITY: \"\"",
    "        CODE_SIGN_ENTITLEMENTS: \"\"",
    "",
  ];
  fs.writeFileSync(`${gen}/passkeys.yml`, lines.join("\n"));
  const file = `${gen}/project.yml`;
  let text = fs.readFileSync(file, "utf8");
  if (!text.includes("passkeys.yml")) text = "include:\n  - passkeys.yml\n" + text;
  // The app depends on the extension, which puts it into PlugIns.
  const target = text.match(/^  ([^\s:]+_iOS):\s*$/m);
  if (!target) { console.error("::error::No iOS app target in project.yml"); process.exit(1); }
  const start = target.index + target[0].length;
  const next = text.slice(start).search(/^  \S/m);
  const end = next < 0 ? text.length : start + next;
  let block = text.slice(start, end);
  const listed = block.match(/^    dependencies:[ \t]*\n([ \t]*)- /m);
  const indent = listed ? listed[1] : "      ";
  const entry = `${indent}- target: UwULockPasskeys\n${indent}  embed: true\n`;
  if (!block.includes("UwULockPasskeys")) {
    if (/^    dependencies:\s*$/m.test(block)) {
      block = block.replace(/^    dependencies:\s*\n/m, (m) => m + entry);
    } else {
      block = block.replace(/\n*$/, "\n") + "    dependencies:\n" + entry;
    }
  }
  text = text.slice(0, start) + block + text.slice(end);
  fs.writeFileSync(file, text);
  console.log(`AutoFill extension added to ${target[1]}`);
' "$gen" "${UWULOCK_IOS_SHORT:-${version%%-*}}" "${UWULOCK_IOS_BUILD:-1}"
(cd "$gen" && xcodegen generate)
merge_info_plist

mkdir -p "$out"

show_rust_log() {
  echo "--- what the Rust build phase printed ---"
  cat "$gen/rust-build.log" 2>/dev/null || echo "(no log written)"
}

# The simulator build, which the smoke test starts afterwards.
if [ "$which" != iphone ]; then
  pnpm tauri ios build --ci --no-sign --target aarch64-sim || { show_rust_log; exit 1; }
  sim=$(find "$gen/build" -type d -name '*.app' -path '*sim*' 2>/dev/null | head -n 1)
  test -n "$sim" || { echo "::error::The simulator build produced no app"; exit 1; }
  rm -rf "$out/simulator"
  mkdir -p "$out/simulator"
  cp -R "$sim" "$out/simulator/"
  echo "Simulator app: $sim"
  test -d "$sim/PlugIns/UwULockPasskeys.appex" || { echo "::error::The AutoFill extension isn't in the simulator app"; exit 1; }
  [ "$which" = simulator ] && { ls -la "$out"; exit 0; }
fi

# The iPhone itself, archived and left unsigned. The .ipa is packed below, the
# same way for every Tauri version.
pnpm tauri ios build --ci --no-sign --target aarch64 || { show_rust_log; exit 1; }
app=$(find "$gen/build" -type d -name '*.app' -not -path '*sim*' 2>/dev/null | head -n 1)
if [ -z "$app" ]; then
  show_rust_log
  echo "::error::The iPhone build produced no app"
  exit 1
fi
echo "iPhone app: $app"
rm -rf "$RUNNER_TEMP/Payload"
mkdir -p "$RUNNER_TEMP/Payload"
cp -R "$app" "$RUNNER_TEMP/Payload/"
(cd "$RUNNER_TEMP" && zip -qry "unsigned.ipa" Payload)
mv "$RUNNER_TEMP/unsigned.ipa" "$out/UwULock-$version-unsigned.ipa"
rm -rf "$RUNNER_TEMP/Payload"

# What ended up inside, so a missing Info.plist key shows in the log.
test -d "$app/PlugIns/UwULockPasskeys.appex" || { echo "::error::The AutoFill extension isn't in the app"; exit 1; }
echo "--- Info.plist of $app ---"
plutil -p "$app/Info.plist" |
  grep -E "CFBundleIdentifier|CFBundleShortVersionString|CFBundleVersion|MinimumOSVersion|NSFaceIDUsageDescription|UIFileSharingEnabled" || true
ls -la "$out"
