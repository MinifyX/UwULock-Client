#!/usr/bin/env bash
# Builds the AutoFill extension for passkeys on macOS 14+ (UwULockPasskeys.appex, universal) from
# apps/desktop/src-tauri/apple/PasskeyProvider — the same Swift as the iPhone's extension.
#
# Every macOS build compiles it, so the Swift can't rot. It only goes into UwULock.app when the
# build is signed with an Apple developer team (UWULOCK_APPLE_TEAM_ID, and APPLE_SIGNING_IDENTITY
# for the certificate): without the team's App Group and Keychain group the extension can't reach
# the vault, and macOS would list an extension that can't do anything. docs/passkeys.md.
#
# Usage: scripts/macos-passkeys.sh <version> <output folder>   → <output folder>/UwULockPasskeys.appex
set -euo pipefail

version="$1"
out="$2"
src="apps/desktop/src-tauri/apple/PasskeyProvider"
name="UwULockPasskeys"
appex="$out/$name.appex"
team="${UWULOCK_APPLE_TEAM_ID:-}"
sdk="$(xcrun --sdk macosx --show-sdk-path)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

rm -rf "$appex"
mkdir -p "$appex/Contents/MacOS"

for arch in arm64 x86_64; do
  # An app extension has no main(): NSExtensionMain starts it.
  xcrun swiftc -O -parse-as-library -application-extension \
    -module-name "$name" -target "$arch-apple-macos14.0" -sdk "$sdk" \
    -Xlinker -e -Xlinker _NSExtensionMain \
    "$src"/*.swift -o "$work/$name-$arch"
done
lipo -create "$work/$name-arm64" "$work/$name-x86_64" -output "$appex/Contents/MacOS/$name"

group=""
prefix=""
if [ -n "$team" ]; then
  group="$team.app.uwulock"
  prefix="$team."
fi
sed -e "s/\$(EXECUTABLE_NAME)/$name/" \
  -e "s/\$(PRODUCT_BUNDLE_IDENTIFIER)/app.uwulock.passkeys/" \
  -e "s/\$(MARKETING_VERSION)/${version%%-*}/" \
  -e "s/\$(CURRENT_PROJECT_VERSION)/$version/" \
  -e "s/\$(UWULOCK_APP_GROUP)/$group/" \
  -e "s/\$(AppIdentifierPrefix)/$prefix/" \
  "$src/Info.plist" >"$appex/Contents/Info.plist"
plutil -insert LSMinimumSystemVersion -string 14.0 "$appex/Contents/Info.plist"
plutil -lint "$appex/Contents/Info.plist"

if [ -n "$team" ] && [ -n "${APPLE_SIGNING_IDENTITY:-}" ]; then
  sed "s/TEAMID/$team/g" "$src/$name-macOS.entitlements" >"$work/entitlements.plist"
  codesign --force --options runtime --timestamp --entitlements "$work/entitlements.plist" \
    --sign "$APPLE_SIGNING_IDENTITY" "$appex"
else
  # Ad-hoc, like the app: sealed, but nobody's.
  codesign --force --sign - "$appex"
fi
codesign --verify --strict "$appex"
lipo "$appex/Contents/MacOS/$name" -verify_arch arm64 x86_64
echo "Built $appex"
