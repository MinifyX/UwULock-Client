#!/usr/bin/env bash
# Starts the simulator build and checks the basics: UwULock comes up, the web
# view draws its first screen and nothing crashes. Screenshots and logs land in
# $2. Waits for the actual event instead of fixed pauses. (Modelled on
# UwUMail's ios-smoke.sh.)
#
# Usage: scripts/ios-smoke.sh <UwULock.app> [output folder]
#        scripts/ios-smoke.sh --boot   only start booting the simulator, which
#                                      takes minutes; CI does it before the build
set -uo pipefail

app="$1"
out="${2:-smoke}"
bundle="app.uwulock"
[ "$app" = "--boot" ] || mkdir -p "$out"
failed=0

fail() {
  echo "::error::$1"
  failed=1
}

# until_ok <seconds> <command...>: runs the command every second until it succeeds.
until_ok() {
  local limit=$1
  shift
  for _ in $(seq "$limit"); do
    "$@" && return 0
    sleep 1
  done
  return 1
}

# The newest iPhone the runner has a runtime for.
device=$(xcrun simctl list devices available -j | node -e '
  let raw = ""; process.stdin.on("data", (d) => (raw += d)).on("end", () => {
    const { devices } = JSON.parse(raw);
    const version = (key) => (key.match(/iOS-(\d+)-(\d+)/) ?? []).slice(1).map(Number);
    const runtimes = Object.keys(devices)
      .filter((key) => key.includes("iOS") && devices[key].some((d) => d.name.includes("iPhone")))
      .sort((a, b) => {
        const [aMajor = 0, aMinor = 0] = version(a);
        const [bMajor = 0, bMinor = 0] = version(b);
        return aMajor - bMajor || aMinor - bMinor;
      });
    const runtime = runtimes.at(-1);
    if (!runtime) { console.error("no iOS runtime with an iPhone"); process.exit(1); }
    const phone = devices[runtime].find((d) => d.name.includes("iPhone"));
    console.error(`${phone.name} on ${runtime}`);
    console.log(phone.udid);
  });
')
[ -n "$device" ] || { fail "No iPhone simulator on this runner"; exit 1; }

# Booting goes on in the background; the run below waits for it to finish.
xcrun simctl boot "$device" 2>/dev/null
if [ "$app" = "--boot" ]; then
  echo "Booting $device"
  exit 0
fi
xcrun simctl bootstatus "$device" -b > /dev/null
xcrun simctl install "$device" "$app" || { fail "Install failed"; exit 1; }
trap 'xcrun simctl shutdown "$device" 2>/dev/null' EXIT

# Everything UwULock prints goes to the system log; there is no terminal to
# hand it to (--console-pty needs a TTY, which CI has not).
xcrun simctl spawn "$device" log stream --style compact --predicate 'processImagePath CONTAINS "UwULock"' \
  > "$out/console.txt" 2>&1 &
logger=$!
trap 'kill "$logger" 2>/dev/null; xcrun simctl shutdown "$device" 2>/dev/null' EXIT

xcrun simctl launch "$device" "$bundle" > "$out/launch.txt" 2>&1 || { fail "UwULock didn't start"; exit 1; }

# The web view ran its JavaScript: WebKit leaves its data in the app's container.
container=$(xcrun simctl get_app_container "$device" "$bundle" data 2>/dev/null)
webkit_data() { [ -n "$container" ] && [ -n "$(find "$container" -path '*WebKit*' -type f 2>/dev/null | head -n 1)" ]; }
if until_ok 60 webkit_data; then
  echo "The web view left its data behind"
else
  echo "::warning::No WebKit data — the web view may not have drawn anything (see the screenshot)"
fi
xcrun simctl io "$device" screenshot "$out/1-start.png" > /dev/null 2>&1
[ -s "$out/1-start.png" ] || fail "No screenshot of the running app"

# Still alive after the start, not quietly gone.
# Simulator apps are processes of the Mac; newer runtimes don't list them in the
# simulator's launchctl any more, so either place counts.
running() {
  xcrun simctl spawn "$device" launchctl list 2>/dev/null | grep -q "$bundle" \
    || pgrep -f "/$(basename "$app")/" > /dev/null
}
if running; then
  echo "UwULock is still running"
else
  fail "UwULock isn't running any more"
fi

# Dark mode: the page follows the system and tells the window (set_appearance).
xcrun simctl ui "$device" appearance dark > /dev/null 2>&1
until_ok 10 running > /dev/null
xcrun simctl io "$device" screenshot "$out/2-dark.png" > /dev/null 2>&1
running || fail "UwULock died when the system switched to dark mode"

if grep -Ei "panicked at|fatal error|Abort trap" "$out/console.txt" > "$out/crashes.txt" 2>/dev/null; then
  fail "Crash in the log, see crashes.txt"
  cat "$out/crashes.txt"
fi

cp ~/Library/Logs/DiagnosticReports/*.ips "$out/" 2>/dev/null
tail -n 200 "$out/console.txt" > "$out/uwulock-log.txt" 2>/dev/null

exit $failed
