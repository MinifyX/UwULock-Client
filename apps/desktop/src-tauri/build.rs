fn main() {
    // UwULock runs from a folder the user can write to, and gets started from
    // wherever a shortcut points: linked DLLs come from System32 only, never
    // from the app's own folder. The runtime half is in `lib.rs`.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        println!("cargo:rustc-link-arg-bins=/DEPENDENTLOADFLAG:0x800");
    }
    // `self_update`: a computer's copy with its own updater (feature `self-update`, the default).
    // Phones and the App Store builds have none (Cargo.toml).
    println!("cargo::rustc-check-cfg=cfg(self_update)");
    let target = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if std::env::var_os("CARGO_FEATURE_SELF_UPDATE").is_some()
        && !matches!(target.as_str(), "android" | "ios")
    {
        println!("cargo:rustc-cfg=self_update");
    }
    tauri_build::build()
}
