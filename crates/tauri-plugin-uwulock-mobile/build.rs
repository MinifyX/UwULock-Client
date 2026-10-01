// Only Rust calls this plugin (`run_mobile_plugin`), never the page: no
// commands, so no permissions to hand out.
const COMMANDS: &[&str] = &[];

fn main() {
    tauri_plugin::Builder::new(COMMANDS)
        .android_path("android")
        .ios_path("ios")
        .build();
}
