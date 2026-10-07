#!/usr/bin/env bash
# Builds the Safari extension for macOS (UwULockSafari.appex, universal): the native part from
# apps/desktop/src-tauri/apple/SafariExtension, and the extension itself — the same as in Chrome
# and Firefox — from apps/extension/dist/safari (pnpm --filter @uwulock/extension build, or the
# artifact CI passes on). docs/extension.md.
#
# The bundle id has to start with the app's: app.uwulock.desktop.safari in the disk image's
# app, app.uwulock.safari in the Mac App Store's.
#
# Signed with APPLE_SIGNING_IDENTITY when it is set (Developer ID or Apple Distribution, with
# the hardened runtime), ad-hoc otherwise. Safari only loads it from an app signed by a
# developer: an ad-hoc one shows up only with "Allow unsigned extensions" (Safari's Develop menu).
#
# Usage: scripts/macos-safari.sh <version> <output folder> <bundle id> [extension folder]
#   → <output folder>/UwULockSafari.appex
set -euo pipefail

version="$1"
out="$2"
id="$3"
files="${4:-apps/extension/dist/safari}"
src="apps/desktop/src-tauri/apple/SafariExtension"
name="UwULockSafari"
appex="$out/$name.appex"
sdk="$(xcrun --sdk macosx --show-sdk-path)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

test -f "$files/manifest.json" || {
  echo "::error::No Safari extension in $files: pnpm --filter @uwulock/extension build:all first"
  exit 1
}
grep -q '"safari"' "$files/manifest.json" || { echo "::error::$files isn't the Safari build"; exit 1; }

rm -rf "$appex"
mkdir -p "$appex/Contents/MacOS" "$appex/Contents/Resources"

for arch in arm64 x86_64; do
  # An app extension has no main(): NSExtensionMain starts it. Safari 17 needs macOS 13.
  xcrun swiftc -O -parse-as-library -application-extension \
    -module-name "$name" -target "$arch-apple-macos13.0" -sdk "$sdk" \
    -Xlinker -e -Xlinker _NSExtensionMain \
    "$src"/*.swift -o "$work/$name-$arch"
done
lipo -create "$work/$name-arm64" "$work/$name-x86_64" -output "$appex/Contents/MacOS/$name"

sed -e "s/\$(EXECUTABLE_NAME)/$name/" \
  -e "s/\$(PRODUCT_BUNDLE_IDENTIFIER)/$id/" \
  -e "s/\$(MARKETING_VERSION)/${version%%-*}/" \
  -e "s/\$(CURRENT_PROJECT_VERSION)/$version/" \
  "$src/Info.plist" >"$appex/Contents/Info.plist"
plutil -insert LSMinimumSystemVersion -string 13.0 "$appex/Contents/Info.plist"
plutil -lint "$appex/Contents/Info.plist"
cp -R "$files"/. "$appex/Contents/Resources/"
cp "$src/PrivacyInfo.xcprivacy" "$appex/Contents/Resources/"

if [ -n "${APPLE_SIGNING_IDENTITY:-}" ]; then
  codesign --force --options runtime --timestamp --entitlements "$src/$name-macOS.entitlements" \
    --sign "$APPLE_SIGNING_IDENTITY" "$appex"
else
  # Ad-hoc, like the unsigned app: sealed, but nobody's.
  codesign --force --sign - --entitlements "$src/$name-macOS.entitlements" "$appex"
fi
codesign --verify --strict "$appex"
lipo "$appex/Contents/MacOS/$name" -verify_arch arm64 x86_64
echo "Built $appex ($id)"
