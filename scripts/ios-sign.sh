#!/usr/bin/env bash
# Signs the unsigned iPhone build (scripts/ios-build.sh) for the App Store: the
# AutoFill extension and the app, each with its App Store provisioning profile
# and its entitlements, packed into an .ipa App Store Connect takes.
#
# The build itself stays unsigned and holds no secret; this runs afterwards, on
# a runner that built nothing (ios.yml, job `testflight`). The same split as the
# Mac App Store build (scripts/build-mas.mjs --sign).
#
# What changes on the way, in both Info.plists:
#   CFBundleVersion            the build number (BUILD_NUMBER); App Store Connect
#                              refuses one it has seen for this version
#   UwULockKeychainGroup       <team>.app.uwulock.passkeys — the unsigned build
#                              knows no team, so Xcode left the prefix empty
#   UwULockAppGroup            group.app.uwulock
#
# Usage: scripts/ios-sign.sh <unsigned.ipa> <profiles folder> <signed.ipa>
#   The profiles folder holds app.uwulock.mobileprovision and
#   app.uwulock.passkeys.mobileprovision (node scripts/asc.mjs profiles ios …).
# Environment: APPLE_TEAM_ID, APPLE_SIGNING_IDENTITY (the "Apple Distribution"
# certificate, in a keychain codesign can reach), BUILD_NUMBER.
set -euo pipefail

ipa="$1"
profiles="$2"
signed="$3"
team="${APPLE_TEAM_ID:?APPLE_TEAM_ID is not set}"
identity="${APPLE_SIGNING_IDENTITY:?APPLE_SIGNING_IDENTITY is not set}"
build="${BUILD_NUMBER:?BUILD_NUMBER is not set}"
src="$(cd "$(dirname "$0")/.." && pwd)/apps/desktop/src-tauri"
group="group.app.uwulock"
keychain="$team.app.uwulock.passkeys"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
unzip -q "$ipa" -d "$work"
app=$(find "$work/Payload" -maxdepth 1 -name '*.app' | head -n 1)
test -n "$app" || { echo "::error::No app in $ipa"; exit 1; }
appex="$app/PlugIns/UwULockPasskeys.appex"
test -d "$appex" || { echo "::error::The AutoFill extension isn't in the app"; exit 1; }

set_key() { # <plist> <key> <value>
  /usr/libexec/PlistBuddy -c "Set :$2 $3" "$1" 2>/dev/null || /usr/libexec/PlistBuddy -c "Add :$2 string $3" "$1"
}
for plist in "$app/Info.plist" "$appex/Info.plist"; do
  set_key "$plist" CFBundleVersion "$build"
  set_key "$plist" UwULockKeychainGroup "$keychain"
  set_key "$plist" UwULockAppGroup "$group"
done
test -f "$app/PrivacyInfo.xcprivacy" || cp "$src/apple/PrivacyInfo.xcprivacy" "$app/"

# The entitlements: what the profiles allow, each named in full. The app has its own Keychain
# group first (the default for its own items, as the unsigned app had) and the extension's
# second. Both carry the AutoFill entitlement: App Store Connect refuses an app whose AutoFill
# extension's container lacks it (ITMS-90729), so the App ID app.uwulock has the capability too.
entitlements() { # <file> <bundle id> <autofill: yes|no> <keychain groups...>
  local file="$1" id="$2" autofill="$3"
  shift 3
  {
    echo '<?xml version="1.0" encoding="UTF-8"?>'
    echo '<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">'
    echo '<plist version="1.0"><dict>'
    echo "<key>application-identifier</key><string>$team.$id</string>"
    echo "<key>com.apple.developer.team-identifier</key><string>$team</string>"
    echo '<key>get-task-allow</key><false/>'
    echo '<key>beta-reports-active</key><true/>'
    echo "<key>com.apple.security.application-groups</key><array><string>$group</string></array>"
    echo '<key>keychain-access-groups</key><array>'
    for g in "$@"; do echo "<string>$g</string>"; done
    echo '</array>'
    if [ "$autofill" = yes ]; then
      echo '<key>com.apple.developer.authentication-services.autofill-credential-provider</key><true/>'
    fi
    echo '</dict></plist>'
  } >"$file"
  plutil -lint "$file" >/dev/null
}
entitlements "$work/app.plist" app.uwulock yes "$team.app.uwulock" "$keychain"
entitlements "$work/appex.plist" app.uwulock.passkeys yes "$keychain"

cp "$profiles/app.uwulock.mobileprovision" "$app/embedded.mobileprovision"
cp "$profiles/app.uwulock.passkeys.mobileprovision" "$appex/embedded.mobileprovision"

# Inside out: whatever code the app carries besides its own program, then the extension, then
# the app, whose signature seals the others.
sign() { codesign --force --timestamp=none --generate-entitlement-der --sign "$identity" "$@"; }
find "$app" \( -name '*.framework' -o -name '*.dylib' \) -not -path '*/PlugIns/*' -print0 |
  while IFS= read -r -d '' nested; do sign "$nested"; done
sign --entitlements "$work/appex.plist" "$appex"
sign --entitlements "$work/app.plist" "$app"

codesign --verify --deep --strict --verbose=2 "$app"
echo "--- entitlements of $(basename "$app") ---"
codesign -d --entitlements - --xml "$app" | plutil -p - 2>/dev/null || true
echo "--- entitlements of $(basename "$appex") ---"
codesign -d --entitlements - --xml "$appex" | plutil -p - 2>/dev/null || true
plutil -p "$app/Info.plist" | grep -E 'CFBundleIdentifier|CFBundleShortVersionString|CFBundleVersion|UwULock'

mkdir -p "$(dirname "$signed")"
rm -f "$signed"
(cd "$work" && zip -qry signed.ipa Payload)
mv "$work/signed.ipa" "$signed"
echo "Signed: $signed"
