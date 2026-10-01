#!/usr/bin/env bash
# Installs the APK on a running emulator and checks the basics: UwULock starts
# without crashing and draws its first screen, it survives being sent to the
# background and back, and the back button puts it away instead of closing it.
# Logs and the view hierarchy land in $2. No screenshots: UwULock forbids them
# (FLAG_SECURE), they would only be black.
#
# Waits for the actual event instead of fixed pauses; each wait gives up after
# its timeout. (Modelled on UwUMail's android-smoke.sh.)
set -uo pipefail

apk="$1"
out="${2:-smoke}"
package="app.uwulock"
mkdir -p "$out"
failed=0

fail() {
  echo "::error::$1"
  failed=1
}

pid() { adb shell pidof "$package" | tr -d '\r'; }

# until <seconds> <command...>: runs the command every second until it succeeds.
until_ok() {
  local limit=$1
  shift
  for _ in $(seq "$limit"); do
    "$@" && return 0
    sleep 1
  done
  return 1
}

foreground() { adb shell dumpsys activity activities | grep -m1 -E "topResumedActivity|mResumedActivity" | grep -q "$package/"; }
not_foreground() { ! foreground; }
dump_ui() {
  adb shell uiautomator dump /sdcard/ui.xml > /dev/null 2>&1 \
    && adb pull /sdcard/ui.xml "$out/$1.xml" > /dev/null 2>&1
}
# The web view's text shows up in the view hierarchy once the page has drawn its first screen.
ui_has_text() { dump_ui "$1" && grep -q 'text="[^"]\+"' "$out/$1.xml" && grep -q "$package" "$out/$1.xml"; }

adb logcat -c
adb install -r "$apk" || { fail "Install failed"; exit 1; }

adb shell am start -W -n "$package/.MainActivity"
until_ok 30 test -n "$(pid)" || fail "UwULock isn't running after the start"
first=$(pid)
until_ok 60 ui_has_text 1-start || echo "::warning::No text from UwULock's page in the view hierarchy (the web view may hide it)"

# Background and back: same process, still alive.
adb shell input keyevent KEYCODE_HOME
until_ok 15 not_foreground || fail "Home didn't put UwULock away"
adb shell am start -W -n "$package/.MainActivity"
until_ok 15 foreground || fail "UwULock didn't come back to the front"
[ "$(pid)" = "$first" ] || fail "UwULock restarted when coming back from the background"

# Back on the first screen puts UwULock away instead of closing it.
adb shell input keyevent KEYCODE_BACK
until_ok 15 not_foreground || fail "Back didn't put UwULock away"
[ -n "$(pid)" ] || fail "Back closed UwULock"
adb shell am start -W -n "$package/.MainActivity"
until_ok 15 foreground || fail "UwULock didn't come back after back"

adb logcat -d > "$out/logcat.txt"
if grep -E "FATAL EXCEPTION|UnsatisfiedLinkError|panicked at|SIGABRT|Abort message" "$out/logcat.txt" \
  | grep -v "com.android.systemui" > "$out/crashes.txt"; then
  fail "Crash in the log, see crashes.txt"
  cat "$out/crashes.txt"
fi
grep -iE "RustStdoutStderr|uwulock|Tauri" "$out/logcat.txt" | tail -n 200 > "$out/uwulock-log.txt" || true

exit $failed
