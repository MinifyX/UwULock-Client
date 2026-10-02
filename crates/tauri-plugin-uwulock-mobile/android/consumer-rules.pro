# Tauri finds the plugin and its commands by name.
-keep class app.uwulock.mobile.UwuLockMobilePlugin { *; }
-keep class app.uwulock.mobile.*Args { *; }
# Rust finds PasskeyBridge.nativeCall by its JNI name.
-keep class app.uwulock.mobile.PasskeyBridge { *; }
