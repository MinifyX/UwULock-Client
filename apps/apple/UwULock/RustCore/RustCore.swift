// The bridge to the Rust core (crates/uwulock-ffi, UniFFI). The skeleton builds without it:
// scripts/apple-build.sh turns it on (UWULOCK_RUST_CORE=YES → rustcore.yml → RUST_CORE) as soon
// as Frameworks/UwuLockFFI.xcframework and Generated/ exist.
import Foundation

enum RustCore {
    /// Whether this build carries the Rust core.
    static var isAvailable: Bool {
        #if RUST_CORE && canImport(UwuLockFFI)
        true
        #else
        false
        #endif
    }

    /// Called once at launch. Sets up the core (data folder, logging) when it's there.
    static func prepare() {
        #if RUST_CORE && canImport(UwuLockFFI)
        // Milestone 4: hand the core the data folder and the Host (events, clipboard,
        // biometrics, opening links) here.
        #endif
    }
}
