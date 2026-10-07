#!/usr/bin/env bash
# Builds UwULock for the simulator and/or for the iPhone, and packs the iPhone
# app into an .ipa.
#
# Nothing here is signed, and the build holds no secret: signing is switched off
# in the generated project before the build. A sideloading tool signs the .ipa
# with your own Apple ID on the way to the phone (docs/mobile.md); for TestFlight
# scripts/ios-sign.sh signs the same .ipa afterwards on another runner
# (docs/app-store.md).
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
    # The credential exchange's activity type (iOS 26), as the SDK spells it: Apple's docs only
    # name the constant (ASCredentialExchangeActivity). Asked of the macOS SDK on this Mac.
    local probe activity
    probe="$(mktemp -d)"
    printf 'import AuthenticationServices\nprint(ASCredentialExchangeActivity)\n' >"$probe/a.swift"
    if xcrun swiftc -sdk "$(xcrun --sdk macosx --show-sdk-path)" -target "$(uname -m)-apple-macos26.0" \
      "$probe/a.swift" -o "$probe/a" 2>/dev/null && activity="$("$probe/a")" && [ -n "$activity" ]; then
      if ! /usr/libexec/PlistBuddy -c "Print :NSUserActivityTypes" "$generated" | grep -qxF "    $activity"; then
        /usr/libexec/PlistBuddy -c "Add :NSUserActivityTypes: string $activity" "$generated"
      fi
      echo "Credential exchange activity: $activity"
    else
      echo "::warning::Couldn't read ASCredentialExchangeActivity from the SDK"
    fi
    rm -rf "$probe"
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
# The app's extensions, each its own target built into the app's PlugIns, unsigned like the app:
#   UwULockPasskeys  AutoFill for passwords and passkeys (apps/desktop/src-tauri/apple/PasskeyProvider,
#                    iOS 17+); it needs a developer team's App Group and Keychain group to reach the
#                    vault (docs/passkeys.md).
#   UwULockSafari    the Safari extension (apps/desktop/src-tauri/apple/SafariExtension): Xcode builds
#                    its native part, the extension's files (apps/extension/dist/safari) are copied
#                    in after the build (docs/extension.md).
node -e '
  const fs = require("fs");
  const [gen, short, build] = process.argv.slice(1);
  const target = (name, dir, id, extra = []) => [
    `  ${name}:`,
    "    type: app-extension",
    "    platform: iOS",
    "    deploymentTarget: \"17.0\"",
    "    sources:",
    `      - path: ../../apple/${dir}`,
    "        excludes: [\"*.entitlements\", \"Info.plist\"]",
    "    settings:",
    "      base:",
    `        PRODUCT_NAME: ${name}`,
    `        PRODUCT_MODULE_NAME: ${name}`,
    `        PRODUCT_BUNDLE_IDENTIFIER: ${id}`,
    `        INFOPLIST_FILE: ../../apple/${dir}/Info.plist`,
    ...extra,
    `        MARKETING_VERSION: "${short}"`,
    `        CURRENT_PROJECT_VERSION: "${build}"`,
    "        SWIFT_VERSION: \"5.0\"",
    "        TARGETED_DEVICE_FAMILY: \"1,2\"",
    "        APPLICATION_EXTENSION_API_ONLY: YES",
    "        CODE_SIGNING_ALLOWED: NO",
    "        CODE_SIGNING_REQUIRED: NO",
    "        CODE_SIGN_IDENTITY: \"\"",
    "        CODE_SIGN_ENTITLEMENTS: \"\"",
  ];
  const lines = [
    "targets:",
    ...target("UwULockPasskeys", "PasskeyProvider", "app.uwulock.passkeys", [
      "        UWULOCK_APP_GROUP: group.app.uwulock",
    ]),
    ...target("UwULockSafari", "SafariExtension", "app.uwulock.safari"),
    "",
  ];
  fs.writeFileSync(`${gen}/extensions.yml`, lines.join("\n"));
  const file = `${gen}/project.yml`;
  let text = fs.readFileSync(file, "utf8");
  if (!text.includes("extensions.yml")) text = "include:\n  - extensions.yml\n" + text;
  // The app depends on the extensions, which puts them into PlugIns.
  const app = text.match(/^  ([^\s:]+_iOS):\s*$/m);
  if (!app) { console.error("::error::No iOS app target in project.yml"); process.exit(1); }
  const start = app.index + app[0].length;
  const next = text.slice(start).search(/^  \S/m);
  const end = next < 0 ? text.length : start + next;
  let block = text.slice(start, end);
  const listed = block.match(/^    dependencies:[ \t]*\n([ \t]*)- /m);
  const indent = listed ? listed[1] : "      ";
  const entries = ["UwULockPasskeys", "UwULockSafari"]
    .filter((name) => !block.includes(name))
    .map((name) => `${indent}- target: ${name}\n${indent}  embed: true\n`)
    .join("");
  if (entries) {
    if (/^    dependencies:\s*$/m.test(block)) {
      block = block.replace(/^    dependencies:\s*\n/m, (m) => m + entries);
    } else {
      block = block.replace(/\n*$/, "\n") + "    dependencies:\n" + entries;
    }
  }
  text = text.slice(0, start) + block + text.slice(end);
  fs.writeFileSync(file, text);
  console.log(`Extensions added to ${app[1]}`);
' "$gen" "${UWULOCK_IOS_SHORT:-${version%%-*}}" "${UWULOCK_IOS_BUILD:-1}"
(cd "$gen" && xcodegen generate)
merge_info_plist

mkdir -p "$out"

# The Safari extension's files, into the extension Xcode built: at the top of the .appex, where
# Safari looks for manifest.json on iOS.
safari_files="${UWULOCK_SAFARI_EXTENSION:-apps/extension/dist/safari}"
test -f "$safari_files/manifest.json" || {
  echo "::error::No Safari extension in $safari_files: pnpm --filter @uwulock/extension build:all first"
  exit 1
}
add_safari_files() { # <app>
  local appex="$1/PlugIns/UwULockSafari.appex"
  test -d "$appex" || { echo "::error::The Safari extension isn't in $1"; exit 1; }
  cp -R "$safari_files"/. "$appex/"
  test -f "$appex/manifest.json" || { echo "::error::No manifest.json in $appex"; exit 1; }
}

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
# The privacy manifest (apple/PrivacyInfo.xcprivacy) at the app's top, where iOS and App Store
# Connect look for it. The extension's own comes in as a resource of its target.
cp apps/desktop/src-tauri/apple/PrivacyInfo.xcprivacy "$app/"
add_safari_files "$app"
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
