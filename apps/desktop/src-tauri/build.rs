fn main() {
    // UwULock runs from a folder the user can write to, and gets started from
    // wherever a shortcut points: linked DLLs come from System32 only, never
    // from the app's own folder. The runtime half is in `lib.rs`.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        println!("cargo:rustc-link-arg-bins=/DEPENDENTLOADFLAG:0x800");
    }
    let target = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    // macOS and iOS have the native app (apps/apple); this one only runs there for
    // development and tests, and has no iOS side any more.
    assert!(
        target != "ios",
        "UwULock for iOS is the native app in apps/apple, not this Tauri app"
    );
    // `self_update`: a computer's copy with its own updater (feature `self-update`, the default).
    // Android has none (Cargo.toml).
    println!("cargo::rustc-check-cfg=cfg(self_update)");
    if std::env::var_os("CARGO_FEATURE_SELF_UPDATE").is_some() && target != "android" {
        println!("cargo:rustc-cfg=self_update");
    }
    tauri_build::build()
}
