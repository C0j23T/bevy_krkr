# Android

`bevy_krkr` is a Rust library for a game's Bevy host. `snbk_android` is a smoke-test NativeActivity host that builds the library as a `cdylib`; a game should call `BevyKrkrPlugin` from its own app and merge the storage permission and Android activity configuration into its existing manifest.

Build the native library for arm64 with the NDK dependencies installed:

```sh
tools/android/build-deps.sh --abi arm64-v8a --api 26
export PKG_CONFIG_LIBDIR="$PWD/target/android-deps/arm64-v8a/lib/pkgconfig"
export PKG_CONFIG_ALLOW_CROSS=1
cargo ndk -t arm64-v8a -P 26 -o target/android-libs build --release \
  --no-default-features --features real_ffi,xp3filter,opus,video-bundled,android-native-activity \
  --example snbk_android
```

This produces `target/android-libs/arm64-v8a/libsnbk_android.so`. The APK host must load that library with a NativeActivity entry named `android_main`; [the example manifest](../examples/android/AndroidManifest.xml) shows the test host declaration. The game data is external and is not bundled into the library. The example looks for `/storage/emulated/0/bevy_krkr/game/data.xp3` and mounts the adjacent XP3 archives.

Android 11 and later require All files access to read arbitrary game files from shared storage. The host manifest must declare `android.permission.MANAGE_EXTERNAL_STORAGE`, and the host must send the user to the system's per-app All files access settings. Declaring the permission alone does not grant it. The example keeps this permission flow in its own `storage_access` module, waits while the user is in Settings, and starts the engine after access is granted. Games distributed through Google Play must satisfy its All files access policy; for other distribution channels, prefer the Storage Access Framework or app-specific storage when that fits the game.

The Android backend stores writable engine data under the app's internal files directory. Hosts can install it before engine startup with `set_filesystem(Arc::new(AndroidFileSystem::new(internal_data_path)))`; file access remains behind the `krkr_storage::FileSystem` interface.
