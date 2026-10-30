#!/usr/bin/env bash
# Builds the Rust core for the native Apple app (docs/native-apple-plan.md): crates/uwulock-ffi
# as a static library for iPhones, the iOS Simulator (Apple silicon) and the Mac (Apple silicon
# and Intel in one), the Swift bindings UniFFI generates from it, and both packed as
#
#   apps/apple/Frameworks/UwuLockFFI.xcframework   the library + the C module "UwuLockFFI"
#   apps/apple/Generated/uwulock_ffi.swift          the Swift side, compiled into the app
#
# scripts/apple-build.sh runs it first. It does nothing while both are newer than the Rust
# sources they come from (the crates, Cargo.toml, Cargo.lock, this script); --force builds anyway.
#
# Usage: scripts/apple-xcframework.sh [--force]
#   UWULOCK_FFI_PROFILE: release (default; the workspace's: LTO, one codegen unit) or dev
#   (quicker to build, a far bigger and slower library: only for trying things out).
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
app="$root/apps/apple"
framework="$app/Frameworks/UwuLockFFI.xcframework"
generated="$app/Generated"
stamp="$app/Frameworks/.UwuLockFFI.stamp"
profile="${UWULOCK_FFI_PROFILE:-release}"
force=false
for arg in "$@"; do
  case "$arg" in
    --force) force=true ;;
    *) echo "Usage: $0 [--force]" >&2; exit 2 ;;
  esac
done

# The sources the library is built from: uwulock-ffi and the workspace crates it uses.
sources=(
  "$root/Cargo.toml" "$root/Cargo.lock" "$0"
  "$root/crates/uwulock-ffi" "$root/crates/uwulock-app" "$root/crates/uwulock-core"
  "$root/crates/uwulock-bitwarden/src" "$root/crates/uwulock-bitwarden/Cargo.toml"
  "$root/crates/uwulock-authenticator/src" "$root/crates/uwulock-authenticator/Cargo.toml"
)
if ! $force && [ -f "$stamp" ] && [ -d "$framework" ] && [ -f "$generated/uwulock_ffi.swift" ] &&
  [ "$(cat "$stamp")" = "$profile" ] &&
  [ -z "$(find "${sources[@]}" -newer "$stamp" -type f -not -path '*/tests/*' -print -quit)" ]; then
  echo "--- Rust core: up to date ($framework)"
  exit 0
fi

command -v cargo >/dev/null && command -v rustup >/dev/null || export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
export CARGO_INCREMENTAL=0
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-3}"
# Older than the app's own (27.0): the library links into it without a version warning.
export IPHONEOS_DEPLOYMENT_TARGET=17.0
export MACOSX_DEPLOYMENT_TARGET=14.0

targets=(aarch64-apple-ios aarch64-apple-ios-sim aarch64-apple-darwin x86_64-apple-darwin)
installed="$(rustup target list --installed)"
for target in "${targets[@]}"; do
  grep -qx "$target" <<<"$installed" || rustup target add "$target"
done

out="$root/target"
dir="$profile"
[ "$profile" = dev ] && dir=debug
args=(-p uwulock-ffi --lib --profile "$profile")
for target in "${targets[@]}"; do
  echo "--- cargo build: $target ($profile)"
  cargo build "${args[@]}" --target "$target" --quiet
done
lib() { echo "$out/$1/$dir/libuwulock_ffi.a"; }

staging="$out/uwulock-ffi-apple"
rm -rf "$staging"
mkdir -p "$staging/headers" "$staging/macos" "$staging/swift"

echo "--- Swift bindings"
cargo run -p uwulock-ffi --features bindgen --bin uniffi-bindgen-swift --quiet -- \
  --swift-sources --headers --modulemap \
  --module-name UwuLockFFI --modulemap-filename module.modulemap \
  "$(lib aarch64-apple-darwin)" "$staging/swift"
mv "$staging/swift/"*.h "$staging/swift/module.modulemap" "$staging/headers/"

echo "--- XCFramework"
lipo -create "$(lib aarch64-apple-darwin)" "$(lib x86_64-apple-darwin)" \
  -output "$staging/macos/libuwulock_ffi.a"
rm -rf "$framework"
mkdir -p "$app/Frameworks" "$generated"
xcodebuild -create-xcframework \
  -library "$(lib aarch64-apple-ios)" -headers "$staging/headers" \
  -library "$(lib aarch64-apple-ios-sim)" -headers "$staging/headers" \
  -library "$staging/macos/libuwulock_ffi.a" -headers "$staging/headers" \
  -output "$framework" >/dev/null
rm -f "$generated/"*.swift
cp "$staging/swift/"*.swift "$generated/"
echo "$profile" >"$stamp"
echo "$framework"
echo "$generated/$(cd "$staging/swift" && ls *.swift)"
