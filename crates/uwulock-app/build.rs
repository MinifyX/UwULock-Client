fn main() {
    // `mobile` and `desktop`, as Tauri's build script sets them for the app:
    // the code moved here keeps its `#[cfg(mobile)]`/`#[cfg(desktop)]`.
    println!("cargo::rustc-check-cfg=cfg(mobile)");
    println!("cargo::rustc-check-cfg=cfg(desktop)");
    let target = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if matches!(target.as_str(), "android" | "ios") {
        println!("cargo:rustc-cfg=mobile");
    } else {
        println!("cargo:rustc-cfg=desktop");
    }
}
