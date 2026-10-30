#!/usr/bin/env bash
# The native Apple app's release builds (apps/apple), for CI (.github/workflows/apple.yml) and a
# Mac at hand alike. Three products from one Xcode project:
#
#   ios     the iPhone/iPad app (app.uwulock): App Store signed .ipa -> TestFlight
#   mas     the Mac App Store app (app.uwulock): signed installer .pkg -> TestFlight
#   direct  the disk image (app.uwulock.desktop): Developer ID, hardened runtime, notarized and
#           stapled, universal (Apple silicon + Intel), updates itself with Sparkle
#
# Building and signing are two steps on two machines in CI: `archive` builds and holds no secret
# (built unsigned, then ad-hoc signed with the team's entitlements in place); `export` signs on a
# runner that builds nothing (Xcode's exportArchive with the certificates and profiles).
# docs/app-store.md.
#
#   scripts/apple-release.sh archive <ios|mas|direct> <out.xcarchive>
#       xcodegen + xcodebuild archive. UWULOCK_VERSION (the release, "0.8.0-beta.1"),
#       BUILD_NUMBER (CFBundleVersion, grows with every build). Xcode builds unsigned (it won't
#       sign for a team without a profile); APPLE_TEAM_ID set: every bundle is then ad-hoc signed
#       with its entitlements, $(TeamIdentifierPrefix) filled in, which exportArchive carries
#       over, ready for `export`; unset: left unsigned (only for `ipa`).
#       The store builds get the version without its "-beta.1" (App Store Connect takes numbers
#       only), the disk image the full one. The Rust core comes from
#       scripts/apple-xcframework.sh (universal for the Mac), or as it is with UWULOCK_FFI_PREBUILT=1.
#   scripts/apple-release.sh ipa <ios.xcarchive> <out.ipa>
#       The app from an (unsigned) iOS archive as an .ipa, for sideloading (AltStore, Sideloadly).
#   scripts/apple-release.sh export <archive> <ios|mas|direct> <profiles folder> <out folder>
#       Signs the archive: the profiles (node scripts/asc.mjs profiles <ios|macos|direct> …)
#       installed, ExportOptions.plist written, xcodebuild -exportArchive. Leaves UwULock.ipa,
#       UwULock.pkg or UwULock.app in <out folder>. Needs APPLE_TEAM_ID and the certificates in
#       a keychain (`keychain` / `developer-id`).
#   scripts/apple-release.sh notarize <UwULock.app|file.dmg>
#       Notarizes with the App Store Connect key (`api-key`, ASC_KEY_ID, ASC_ISSUER_ID) and
#       staples the ticket. UWULOCK_SKIP_NOTARIZE=1 skips it (a dry run).
#   scripts/apple-release.sh adhoc <UwULock.app>
#       The disk image's fallback without a Developer ID: signs the app ad-hoc again, without the
#       entitlements only a provisioning profile allows (Keychain groups, App Group, AutoFill), so
#       it starts. It then runs without AutoFill and the shared Keychain.
#   scripts/apple-release.sh dmg <UwULock.app> <out.dmg>
#       The disk image: the app and a link to /Applications. Signed with APPLE_SIGNING_IDENTITY
#       when set (the Developer ID), else left unsigned.
#   scripts/apple-release.sh upload <file.ipa|file.pkg> <ios|macos>
#       Uploads to App Store Connect; TestFlight has it once Apple has processed it.
#
#   scripts/apple-release.sh xcode
#       Selects the newest Xcode 27 on a GitHub runner (/Applications/Xcode_27*.app).
#
# The CI-only steps (GitHub runner, RUNNER_TEMP + GITHUB_ENV):
#   keychain       a keychain with APPLE_DISTRIBUTION_P12 and, when set, APPLE_INSTALLER_P12
#                  (base64, password APPLE_P12_PASSWORD). Sets KEYCHAIN, APPLE_SIGNING_IDENTITY,
#                  APPLE_INSTALLER_IDENTITY and SIGNING_CERT (the certificate as PEM, for asc.mjs).
#   developer-id   the same with the Developer ID Application certificate, from
#                  APPLE_DEVELOPER_ID_KEY (private key, PEM) and APPLE_DEVELOPER_ID_CERT (PEM or
#                  base64 DER). Sets KEYCHAIN, APPLE_SIGNING_IDENTITY and SIGNING_CERT.
#   api-key        the App Store Connect key from APPLE_ASC_KEY_P8 as a file. Sets ASC_KEY_PATH
#                  and API_PRIVATE_KEYS_DIR.
#   cleanup        removes keychain, key and profiles, however the job ended.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
apple="$root/apps/apple"
derived="${UWULOCK_DERIVED_DATA:-$apple/build/DerivedData}"

die() { echo "::error::$*" >&2; exit 1; }
# Apple silicon and Intel. (Xcode 27's `lipo -verify_arch` takes one architecture at a time.)
universal() {
  local archs
  archs=" $(lipo -archs "$1") "
  [[ "$archs" == *" arm64 "* && "$archs" == *" x86_64 "* ]] || die "$1 isn't universal:$archs"
}

# Scheme, configuration, destination and the app's bundle identifier per product.
product() {
  case "$1" in
    ios) scheme=UwULock config=Release destination="generic/platform=iOS" bundle=app.uwulock ;;
    mas) scheme=UwULock config=Release destination="generic/platform=macOS" bundle=app.uwulock ;;
    direct) scheme="UwULock Direct" config=Release-Direct destination="generic/platform=macOS" bundle=app.uwulock.desktop ;;
    *) die "product: ios, mas or direct, not '$1'" ;;
  esac
}

# --- CI keychain and keys --------------------------------------------------------------------

ci() {
  : "${RUNNER_TEMP:?runs on a GitHub runner}" "${GITHUB_ENV:?runs on a GitHub runner}"
  keychain="$RUNNER_TEMP/uwulock-signing.keychain-db"
  keys="$RUNNER_TEMP/private_keys"
}

# A keychain of this job's own, unlocked, with Apple's intermediate certificates (so codesign
# can build the chain whatever the runner has). Its password stays in $password.
new_keychain() {
  password="$(openssl rand -hex 24)"
  security create-keychain -p "$password" "$keychain"
  security set-keychain-settings -lut 3600 "$keychain"
  security unlock-keychain -p "$password" "$keychain"
  for ca in AppleWWDRCAG3 DeveloperIDG2CA DeveloperIDCA; do
    curl -fsSL --retry 3 -o "$RUNNER_TEMP/$ca.cer" "https://www.apple.com/certificateauthority/$ca.cer"
    security import "$RUNNER_TEMP/$ca.cer" -k "$keychain" >/dev/null || true
    rm -f "$RUNNER_TEMP/$ca.cer"
  done
}

# The keychain ready for codesign and productbuild, and first in the search list.
use_keychain() {
  security set-key-partition-list -S apple-tool:,apple:,codesign:,productbuild: -s -k "$password" "$keychain" >/dev/null
  # shellcheck disable=SC2046 # the existing keychains, one word each
  security list-keychains -d user -s "$keychain" $(security list-keychains -d user | tr -d '"')
}

# --- profiles and ExportOptions --------------------------------------------------------------

# Installs every profile in <folder> where Xcode looks and prints "<bundle id> <uuid>" per profile.
install_profiles() {
  local folder="$1" file plist uuid id team
  local dirs=("$HOME/Library/MobileDevice/Provisioning Profiles" "$HOME/Library/Developer/Xcode/UserData/Provisioning Profiles")
  mkdir -p "${dirs[@]}"
  plist="$(mktemp)"
  for file in "$folder"/*.mobileprovision "$folder"/*.provisionprofile; do
    [ -f "$file" ] || continue
    security cms -D -i "$file" >"$plist"
    uuid="$(/usr/libexec/PlistBuddy -c 'Print :UUID' "$plist")"
    team="$(/usr/libexec/PlistBuddy -c 'Print :TeamIdentifier:0' "$plist")"
    # application-identifier (iOS) or com.apple.application-identifier (macOS): <team>.<bundle id>
    id="$(/usr/libexec/PlistBuddy -c 'Print :Entitlements:application-identifier' "$plist" 2>/dev/null ||
      /usr/libexec/PlistBuddy -c 'Print :Entitlements:com.apple.application-identifier' "$plist")"
    id="${id#"$team".}"
    for dir in "${dirs[@]}"; do cp "$file" "$dir/$uuid.${file##*.}"; done
    echo "$id $uuid"
  done
  rm -f "$plist"
}

export_options() { # <product> <profiles folder> <plist>
  local kind="$1" folder="$2" out="$3" method certificate installer="" id uuid
  local team="${APPLE_TEAM_ID:?APPLE_TEAM_ID is not set}"
  case "$kind" in
    ios) method=app-store-connect certificate="Apple Distribution" ;;
    mas)
      method=app-store-connect certificate="Apple Distribution"
      installer="${APPLE_INSTALLER_IDENTITY:-3rd Party Mac Developer Installer}"
      ;;
    direct) method=developer-id certificate="Developer ID Application" ;;
  esac
  {
    echo '<?xml version="1.0" encoding="UTF-8"?>'
    echo '<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">'
    echo '<plist version="1.0"><dict>'
    echo "<key>method</key><string>$method</string>"
    echo "<key>teamID</key><string>$team</string>"
    echo '<key>signingStyle</key><string>manual</string>'
    echo "<key>signingCertificate</key><string>$certificate</string>"
    [ -z "$installer" ] || echo "<key>installerSigningCertificate</key><string>$installer</string>"
    echo '<key>destination</key><string>export</string>'
    echo '<key>uploadSymbols</key><true/>'
    echo '<key>manageAppVersionAndBuildNumber</key><false/>'
    echo '<key>provisioningProfiles</key><dict>'
    while read -r id uuid; do
      echo "<key>$id</key><string>$uuid</string>"
    done < <(install_profiles "$folder")
    echo '</dict>'
    echo '</dict></plist>'
  } >"$out"
  plutil -lint "$out" >/dev/null
  plutil -p "$out"
}

# --- commands --------------------------------------------------------------------------------

cmd_archive() {
  local kind="${1:?product}" out="${2:?out.xcarchive}" version marketing log
  product "$kind"
  version="${UWULOCK_VERSION:?UWULOCK_VERSION is not set}"
  marketing="$version"
  [ "$kind" = direct ] || marketing="${version%%-*}"
  : "${BUILD_NUMBER:?BUILD_NUMBER is not set}"
  command -v xcodegen >/dev/null || die "xcodegen is missing: brew install xcodegen"

  if [ "${UWULOCK_FFI_PREBUILT:-}" != 1 ]; then
    if [ "$kind" = ios ]; then "$root/scripts/apple-xcframework.sh"; else "$root/scripts/apple-xcframework.sh" --universal; fi
  fi
  [ -d "$apple/Frameworks/UwuLockFFI.xcframework" ] && [ -d "$apple/Generated" ] || die "no Rust core in apps/apple/Frameworks"
  export UWULOCK_RUST_CORE=YES
  xcodegen generate --spec "$apple/project.yml" --project "$apple" --quiet

  # Unsigned: Xcode won't sign the targets with AutoFill, App Group and Keychain groups without
  # their profiles, which only the signing machine has. `entitle` signs them ad-hoc afterwards.
  local signing=(CODE_SIGNING_ALLOWED=NO)
  [ -z "${APPLE_TEAM_ID:-}" ] || signing+=(DEVELOPMENT_TEAM="$APPLE_TEAM_ID")
  local archs=()
  [ "$kind" = ios ] || archs=(ARCHS="arm64 x86_64" ONLY_ACTIVE_ARCH=NO)

  echo "--- archive $kind: $scheme ($config) $marketing ($BUILD_NUMBER)${APPLE_TEAM_ID:+, team $APPLE_TEAM_ID}"
  rm -rf "$out"
  log="$(mktemp)"
  if ! xcodebuild -project "$apple/UwULock.xcodeproj" -scheme "$scheme" -configuration "$config" \
    -destination "$destination" -derivedDataPath "$derived" -archivePath "$out" \
    -skipPackagePluginValidation -skipMacroValidation \
    MARKETING_VERSION="$marketing" CURRENT_PROJECT_VERSION="$BUILD_NUMBER" \
    ${archs[@]+"${archs[@]}"} "${signing[@]}" archive >"$log" 2>&1; then
    grep -E "error:|\*\* ARCHIVE" "$log" | sort -u | head -50
    echo "Full log: $log" >&2
    return 1
  fi
  grep -E "^$apple/.*warning:" "$log" | sort -u | head -20 || true
  grep -E "\*\* ARCHIVE" "$log" | tail -1
  rm -f "$log"

  local app="$out/Products/Applications/UwULock.app" plist
  [ -d "$app" ] || die "no UwULock.app in $out"
  if [ -n "${APPLE_TEAM_ID:-}" ]; then
    entitle "$kind" "$app" ${archs[@]+"${archs[@]}"} MARKETING_VERSION="$marketing" CURRENT_PROJECT_VERSION="$BUILD_NUMBER"
  fi
  [ "$kind" = ios ] && plist="$app/Info.plist" || plist="$app/Contents/Info.plist"
  plutil -extract CFBundleIdentifier raw "$plist" | grep -qx "$bundle" || die "$app isn't $bundle"
  [ "$kind" = ios ] || universal "$app/Contents/MacOS/UwULock"
  echo "$app: $bundle $(plutil -extract CFBundleShortVersionString raw "$plist") ($(plutil -extract CFBundleVersion raw "$plist"))"
}

# Signs an archived app ad-hoc with each target's entitlements, $(TeamIdentifierPrefix) and
# $(AppIdentifierPrefix) resolved for APPLE_TEAM_ID: exportArchive takes the entitlements from
# these signatures when it signs for real (and checks them against the profiles).
entitle() {
  local kind="$1" app="$2" settings sdk name file i=0 bundle runtime=()
  shift 2
  sdk=iphoneos
  if [ "$kind" != ios ]; then sdk=macosx runtime=(--options runtime); fi
  # Each target's entitlements file for this configuration and platform (the scheme alone lists
  # only the app, not the extensions it embeds).
  settings="$(mktemp)"
  xcodebuild -project "$apple/UwULock.xcodeproj" -alltargets -configuration "$config" -sdk "$sdk" \
    "$@" -showBuildSettings -json 2>/dev/null | plutil -convert xml1 -o "$settings" -
  local names=() files=()
  while name="$(/usr/libexec/PlistBuddy -c "Print :$i:buildSettings:FULL_PRODUCT_NAME" "$settings" 2>/dev/null)"; do
    names+=("$name")
    files+=("$(/usr/libexec/PlistBuddy -c "Print :$i:buildSettings:CODE_SIGN_ENTITLEMENTS" "$settings" 2>/dev/null || true)")
    i=$((i + 1))
  done
  rm -f "$settings"
  [ "$i" -gt 0 ] || die "no build settings for $config ($sdk)"
  entitlements_of() {
    local j
    for ((j = 0; j < ${#names[@]}; j++)); do
      [ "${names[j]}" != "$1" ] || { echo "${files[j]}"; return; }
    done
    die "$1: no target builds it"
  }

  # Sparkle's helpers and the frameworks first (no entitlements of their own), then the
  # extensions, then the app, whose signature seals the extensions as they are now.
  codesign --force --deep --sign - ${runtime[@]+"${runtime[@]}"} "$app" 2>&1 | grep -v 'replacing existing signature' || true
  local plugins="$app/Contents/PlugIns"
  [ "$kind" != ios ] || plugins="$app/PlugIns"
  for bundle in "$plugins"/*.appex "$app"; do
    [ -d "$bundle" ] || continue
    file="$(entitlements_of "$(basename "$bundle")")"
    [ -n "$file" ] || continue
    entitle_bundle "$bundle" "$file" ${runtime[@]+"${runtime[@]}"}
  done
  codesign --verify --deep --strict "$app"
}

entitle_bundle() {
  local bundle="$1" file="$2" resolved
  shift 2
  [ -d "$bundle" ] || die "$(basename "$bundle") isn't in the archive"
  resolved="$(mktemp)"
  sed -e "s/\$(TeamIdentifierPrefix)/$APPLE_TEAM_ID./g" -e "s/\$(AppIdentifierPrefix)/$APPLE_TEAM_ID./g" \
    "$apple/$file" >"$resolved"
  # shellcheck disable=SC2016 # a literal $(
  ! grep -qF '$(' "$resolved" || die "$file has a variable entitle() doesn't resolve"
  codesign --force --sign - "$@" --entitlements "$resolved" "$bundle" 2>&1 | grep -v 'replacing existing signature' || true
  echo "  $(basename "$bundle"): $file"
  rm -f "$resolved"
}

cmd_ipa() {
  local archive="${1:?ios.xcarchive}" out="${2:?out.ipa}" work
  [ -d "$archive/Products/Applications/UwULock.app" ] || die "no UwULock.app in $archive"
  work="$(mktemp -d)"
  mkdir -p "$work/Payload" "$(dirname "$out")"
  ditto "$archive/Products/Applications/UwULock.app" "$work/Payload/UwULock.app"
  rm -f "$out"
  (cd "$work" && zip -qry out.ipa Payload)
  mv "$work/out.ipa" "$out"
  rm -rf "$work"
  echo "$out"
}

cmd_export() {
  local archive="${1:?archive}" kind="${2:?product}" profiles="${3:?profiles folder}" out="${4:?out folder}" options log
  product "$kind"
  mkdir -p "$out"
  options="$(mktemp -t ExportOptions).plist"
  export_options "$kind" "$profiles" "$options"
  log="$(mktemp)"
  if ! xcodebuild -exportArchive -archivePath "$archive" -exportPath "$out" -exportOptionsPlist "$options" >"$log" 2>&1; then
    grep -E "error|\*\* EXPORT" "$log" | sort -u | head -40
    echo "Full log: $log" >&2
    return 1
  fi
  grep -E "\*\* EXPORT" "$log" | tail -1
  rm -f "$log" "$options"
  case "$kind" in
    ios) ls "$out"/*.ipa ;;
    mas) ls "$out"/*.pkg ;;
    direct)
      codesign --verify --deep --strict --verbose=2 "$out/UwULock.app"
      codesign -d --verbose=2 "$out/UwULock.app" 2>&1 | grep -E "^(Authority|TeamIdentifier|Runtime Version|flags)" || true
      ;;
  esac
}

cmd_notarize() {
  local file="${1:?app or dmg}" submit
  if [ "${UWULOCK_SKIP_NOTARIZE:-}" = 1 ]; then
    echo "--- notarize: skipped (UWULOCK_SKIP_NOTARIZE=1): $file"
    return 0
  fi
  : "${ASC_KEY_PATH:?run api-key first}" "${ASC_KEY_ID:?}" "${ASC_ISSUER_ID:?}"
  submit="$file"
  if [ -d "$file" ]; then
    submit="$(mktemp -d)/$(basename "$file" .app).zip"
    ditto -c -k --keepParent "$file" "$submit"
  fi
  echo "--- notarize: $file"
  xcrun notarytool submit "$submit" --key "$ASC_KEY_PATH" --key-id "$ASC_KEY_ID" \
    --issuer "$ASC_ISSUER_ID" --wait --timeout 30m
  [ "$submit" = "$file" ] || rm -f "$submit"
  xcrun stapler staple "$file"
  xcrun stapler validate "$file"
}

cmd_adhoc() {
  local app="${1:?UwULock.app}" bundle saved key i=0
  [ -d "$app/Contents" ] || die "no Mac app at $app"
  # The extensions' and the app's entitlements as the archive signed them, less what only a
  # provisioning profile allows.
  saved="$(mktemp -d)"
  local bundles=("$app"/Contents/PlugIns/*.appex "$app")
  for bundle in "${bundles[@]}"; do
    [ -d "$bundle" ] || continue
    codesign -d --entitlements - --xml "$bundle" >"$saved/$i.plist" 2>/dev/null || true
    for key in keychain-access-groups com.apple.security.application-groups \
      com.apple.developer.authentication-services.autofill-credential-provider \
      com.apple.application-identifier com.apple.developer.team-identifier; do
      [ ! -s "$saved/$i.plist" ] || /usr/libexec/PlistBuddy -c "Delete :$key" "$saved/$i.plist" >/dev/null 2>&1 || true
    done
    i=$((i + 1))
  done
  # Everything ad-hoc (Sparkle's helpers, the frameworks), then the extensions and the app again
  # with their entitlements.
  codesign --force --deep --sign - --options runtime "$app" 2>&1 | grep -v 'replacing existing signature' || true
  i=0
  for bundle in "${bundles[@]}"; do
    [ -d "$bundle" ] || continue
    if [ -s "$saved/$i.plist" ]; then
      codesign --force --sign - --options runtime --entitlements "$saved/$i.plist" "$bundle" 2>&1 |
        grep -v 'replacing existing signature' || true
    fi
    i=$((i + 1))
  done
  rm -rf "$saved"
  codesign --verify --deep --strict "$app"
  echo "$app: ad-hoc signed"
}

cmd_xcode() {
  local xcode
  # shellcheck disable=SC2012 # names without spaces
  xcode="$(ls -d /Applications/Xcode_27*.app 2>/dev/null | sort -V | tail -n 1)"
  [ -n "$xcode" ] || { ls -d /Applications/Xcode*.app >&2 || true; die "No Xcode 27 on this runner"; }
  sudo xcode-select --switch "$xcode"
  xcodebuild -version
}

cmd_dmg() {
  local app="${1:?UwULock.app}" out="${2:?out.dmg}" stage
  [ -d "$app" ] || die "no app at $app"
  stage="$(mktemp -d)"
  ditto "$app" "$stage/UwULock.app"
  ln -s /Applications "$stage/Applications"
  mkdir -p "$(dirname "$out")"
  rm -f "$out"
  hdiutil create -quiet -volname UwULock -srcfolder "$stage" -fs APFS -format ULFO "$out"
  rm -rf "$stage"
  if [ -n "${APPLE_SIGNING_IDENTITY:-}" ]; then
    codesign --force --timestamp --sign "$APPLE_SIGNING_IDENTITY" ${KEYCHAIN:+--keychain "$KEYCHAIN"} "$out"
    codesign --verify --strict "$out"
  fi
  echo "$out ($(du -h "$out" | cut -f1))"
}

cmd_upload() {
  local file="${1:?file}" platform="${2:?ios or macos}"
  : "${ASC_KEY_ID:?}" "${ASC_ISSUER_ID:?}" "${API_PRIVATE_KEYS_DIR:?run api-key first}"
  xcrun altool --upload-app --file "$file" --type "$platform" \
    --apiKey "$ASC_KEY_ID" --apiIssuer "$ASC_ISSUER_ID" --output-format normal
}

cmd_keychain() {
  ci
  new_keychain
  for name in APPLE_DISTRIBUTION_P12 APPLE_INSTALLER_P12; do
    [ -n "${!name:-}" ] || continue
    printf '%s' "${!name}" | base64 --decode >"$RUNNER_TEMP/certificate.p12"
    security import "$RUNNER_TEMP/certificate.p12" -k "$keychain" -P "${APPLE_P12_PASSWORD:?}" \
      -T /usr/bin/codesign -T /usr/bin/productbuild -T /usr/bin/security >/dev/null
    rm -f "$RUNNER_TEMP/certificate.p12"
  done
  use_keychain

  local identities app installer
  identities="$(security find-identity -v "$keychain")"
  echo "$identities" | grep -E '^ +[0-9]+\)' | sed -E 's/^ +[0-9]+\) [0-9A-F]+ /  /'
  app=$(echo "$identities" | grep '"Apple Distribution: ' | head -n 1 | awk '{print $2}' || true)
  [ -n "$app" ] || die "No Apple Distribution certificate in APPLE_DISTRIBUTION_P12"
  installer=$(echo "$identities" | grep -E '"(3rd Party Mac Developer Installer|Mac Installer Distribution): ' |
    head -n 1 | sed -E 's/.*"(.*)".*/\1/' || true)
  security find-certificate -c "Apple Distribution: " -p "$keychain" >"$RUNNER_TEMP/signing.pem"
  grep -q 'BEGIN CERTIFICATE' "$RUNNER_TEMP/signing.pem" || die "Couldn't export the certificate"
  {
    echo "KEYCHAIN=$keychain"
    echo "APPLE_SIGNING_IDENTITY=$app"
    echo "SIGNING_CERT=$RUNNER_TEMP/signing.pem"
    [ -z "$installer" ] || echo "APPLE_INSTALLER_IDENTITY=$installer"
  } >>"$GITHUB_ENV"
}

cmd_developer_id() {
  ci
  : "${APPLE_DEVELOPER_ID_KEY:?}" "${APPLE_DEVELOPER_ID_CERT:?}"
  new_keychain
  # Key and certificate as one .p12 for `security import`; macOS's own openssl (LibreSSL)
  # writes the older encryption it reads.
  umask 077
  printf '%s\n' "$APPLE_DEVELOPER_ID_KEY" >"$RUNNER_TEMP/developer-id.key"
  # The certificate as PEM, or as the .cer Apple hands out, in base64.
  if printf '%s' "$APPLE_DEVELOPER_ID_CERT" | grep -q 'BEGIN CERTIFICATE'; then
    printf '%s\n' "$APPLE_DEVELOPER_ID_CERT" >"$RUNNER_TEMP/signing.pem"
  else
    printf '%s' "$APPLE_DEVELOPER_ID_CERT" | base64 --decode |
      /usr/bin/openssl x509 -inform der -out "$RUNNER_TEMP/signing.pem"
  fi
  local p12 identities identity
  p12="$(openssl rand -hex 16)"
  /usr/bin/openssl pkcs12 -export -inkey "$RUNNER_TEMP/developer-id.key" \
    -in "$RUNNER_TEMP/signing.pem" -out "$RUNNER_TEMP/developer-id.p12" -passout "pass:$p12"
  rm -f "$RUNNER_TEMP/developer-id.key"
  security import "$RUNNER_TEMP/developer-id.p12" -k "$keychain" -P "$p12" \
    -T /usr/bin/codesign -T /usr/bin/security >/dev/null
  rm -f "$RUNNER_TEMP/developer-id.p12"
  use_keychain

  identities="$(security find-identity -v -p codesigning "$keychain")"
  echo "$identities" | grep -E '^ +[0-9]+\)' | sed -E 's/^ +[0-9]+\) [0-9A-F]+ /  /'
  identity=$(echo "$identities" | grep '"Developer ID Application: ' | head -n 1 | awk '{print $2}' || true)
  [ -n "$identity" ] || die "No valid Developer ID Application identity: do APPLE_DEVELOPER_ID_KEY and APPLE_DEVELOPER_ID_CERT belong together?"
  {
    echo "KEYCHAIN=$keychain"
    echo "APPLE_SIGNING_IDENTITY=$identity"
    echo "SIGNING_CERT=$RUNNER_TEMP/signing.pem"
  } >>"$GITHUB_ENV"
}

cmd_api_key() {
  ci
  : "${ASC_KEY_ID:?}" "${APPLE_ASC_KEY_P8:?}"
  mkdir -p "$keys"
  chmod 700 "$keys"
  printf '%s\n' "$APPLE_ASC_KEY_P8" >"$keys/AuthKey_$ASC_KEY_ID.p8"
  chmod 600 "$keys/AuthKey_$ASC_KEY_ID.p8"
  {
    echo "ASC_KEY_PATH=$keys/AuthKey_$ASC_KEY_ID.p8"
    echo "API_PRIVATE_KEYS_DIR=$keys"
  } >>"$GITHUB_ENV"
}

cmd_cleanup() {
  ci
  security delete-keychain "$keychain" 2>/dev/null || true
  rm -rf "$keys" "$RUNNER_TEMP/profiles" "$HOME/Library/MobileDevice/Provisioning Profiles" \
    "$HOME/Library/Developer/Xcode/UserData/Provisioning Profiles"
  rm -f "$RUNNER_TEMP/signing.pem" "$RUNNER_TEMP"/developer-id.*
}

command="${1:-}"
[ $# -eq 0 ] || shift
case "$command" in
  archive | ipa | export | notarize | adhoc | dmg | upload | xcode | keychain | api-key | cleanup) "cmd_${command//-/_}" "$@" ;;
  developer-id) cmd_developer_id "$@" ;;
  *)
    echo "Usage: scripts/apple-release.sh archive|ipa|export|notarize|adhoc|dmg|upload|xcode|keychain|developer-id|api-key|cleanup … (see the head of this file)" >&2
    exit 2
    ;;
esac
