#!/usr/bin/env bash
# Builds the native Apple app (apps/apple, docs/native-apple-plan.md): generates the Xcode
# project from project.yml (XcodeGen) and builds it unsigned for the iOS Simulator and the Mac.
# With --screens it also runs the app in the simulator (and on this Mac) and saves screenshots
# to apps/apple/docs-screens/.
#
# The Rust core comes in when scripts/apple-xcframework.sh has built
# apps/apple/Frameworks/UwuLockFFI.xcframework and apps/apple/Generated/; without them the app
# builds on its stub.
#
# Usage: scripts/apple-build.sh [ios] [macos] [direct] [--screens]
#   no targets: ios and macos. direct: the Mac's disk-image build (Debug-Direct, Sparkle).
#   UWULOCK_SIMULATOR: the simulator for --screens (name or UDID; default: newest iPhone).
#   UWULOCK_CONFIGURATION: Debug (default) or Release.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
app="$root/apps/apple"
derived="$app/build/DerivedData"
screens="$app/docs-screens"
config="${UWULOCK_CONFIGURATION:-Debug}"

targets=()
take_screens=false
for arg in "$@"; do
  case "$arg" in
    ios | macos | direct) targets+=("$arg") ;;
    --screens) take_screens=true ;;
    *) echo "Usage: $0 [ios] [macos] [direct] [--screens]" >&2; exit 2 ;;
  esac
done
[ ${#targets[@]} -gt 0 ] || targets=(ios macos)

command -v xcodegen >/dev/null || { echo "xcodegen is missing: brew install xcodegen" >&2; exit 1; }

if [ -d "$app/Frameworks/UwuLockFFI.xcframework" ] && [ -d "$app/Generated" ]; then
  export UWULOCK_RUST_CORE=YES
  echo "--- Rust core: on"
else
  export UWULOCK_RUST_CORE=NO
  echo "--- Rust core: stub (no Frameworks/UwuLockFFI.xcframework yet)"
fi

echo "--- xcodegen"
xcodegen generate --spec "$app/project.yml" --project "$app" --quiet

# Prints only errors, warnings from our own sources and the result: the full log is long.
build() {
  local scheme="$1" configuration="$2" destination="$3" log
  log="$(mktemp)"
  echo "--- build: $scheme ($configuration) for $destination"
  if ! xcodebuild -project "$app/UwULock.xcodeproj" -scheme "$scheme" \
    -configuration "$configuration" -destination "$destination" \
    -derivedDataPath "$derived" -skipPackagePluginValidation -skipMacroValidation \
    CODE_SIGNING_ALLOWED=NO build >"$log" 2>&1; then
    grep -E "error:|\*\* BUILD" "$log" | sort -u | head -50
    echo "Full log: $log" >&2
    return 1
  fi
  grep -E "^$app/.*(warning|error):" "$log" | sort -u | head -30 || true
  grep -E "\*\* BUILD" "$log"
  rm -f "$log"
}

for target in "${targets[@]}"; do
  case "$target" in
    ios) build UwULock "$config" "generic/platform=iOS Simulator" ;;
    macos) build UwULock "$config" "generic/platform=macOS" ;;
    direct) build "UwULock Direct" "$config-Direct" "generic/platform=macOS" ;;
  esac
done

$take_screens || exit 0
mkdir -p "$screens"

# --- iOS Simulator: lock screen, login, vault (light and dark).
if [[ " ${targets[*]} " == *" ios "* ]]; then
  sim="${UWULOCK_SIMULATOR:-}"
  if [ -z "$sim" ]; then
    sim="$(xcrun simctl list devices available -j | node -e '
      const { devices } = JSON.parse(require("fs").readFileSync(0, "utf8"));
      const ios = Object.keys(devices).filter((r) => r.includes("iOS")).sort().reverse();
      for (const runtime of ios) {
        const phone = devices[runtime].find((d) => /^iPhone \d+$/.test(d.name)) ??
          devices[runtime].find((d) => d.name.startsWith("iPhone"));
        if (phone) { console.log(phone.udid); break; }
      }')"
  fi
  [ -n "$sim" ] || { echo "No iPhone simulator found" >&2; exit 1; }
  echo "--- simulator: $sim"
  xcrun simctl boot "$sim" 2>/dev/null || true
  xcrun simctl bootstatus "$sim" -b >/dev/null
  xcrun simctl install "$sim" "$derived/Build/Products/$config-iphonesimulator/UwULock.app"
  shot() {
    local name="$1" phase="$2" appearance="$3"
    xcrun simctl ui "$sim" appearance "$appearance"
    xcrun simctl launch --terminate-running-process "$sim" app.uwulock -UwULockPhase "$phase" >/dev/null
    sleep 3
    xcrun simctl io "$sim" screenshot --type=png "$screens/$name.png" >/dev/null 2>&1
    echo "$screens/$name.png"
  }
  shot ios-lock locked light
  shot ios-login login light
  shot ios-vault unlocked light
  shot ios-vault-dark unlocked dark
  xcrun simctl ui "$sim" appearance light
  xcrun simctl terminate "$sim" app.uwulock 2>/dev/null || true
fi

# --- Mac: the vault window. Needs Screen Recording permission for the terminal; skipped
# (with a note) without it.
if [[ " ${targets[*]} " == *" macos "* ]]; then
  mac_app="$derived/Build/Products/$config/UwULock.app"
  pkill -x UwULock 2>/dev/null || true
  open -n "$mac_app" --args -UwULockPhase unlocked
  sleep 4
  window="$(swift -e '
    import CoreGraphics
    let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] ?? []
    let mine = list.filter { ($0[kCGWindowOwnerName as String] as? String) == "UwULock" && ($0[kCGWindowLayer as String] as? Int) == 0 }
    if let id = mine.first?[kCGWindowNumber as String] as? Int { print(id) }' 2>/dev/null || true)"
  if [ -n "$window" ] && screencapture -o -x -l "$window" "$screens/macos-vault.png" 2>/dev/null; then
    echo "$screens/macos-vault.png"
  else
    echo "Mac screenshot skipped (no window found or no Screen Recording permission)" >&2
  fi
  pkill -x UwULock 2>/dev/null || true
fi
