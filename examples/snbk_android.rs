//! Android build of the `snbk` example: the same app as a `cdylib`.
//!
//! ```sh
//! cargo ndk -t arm64-v8a -P 26 -o target/android-libs build --release \
//!   --no-default-features --features real_ffi,xp3filter,opus,video-bundled \
//!   --example snbk_android
//! ```
//!
//! See README, "Android", for `tools/android/build-deps.sh` and the pkg-config
//! environment the engine core needs. `#[bevy_main]` in `snbk.rs` exports the
//! `android_main` entry point. `target/android-libs/arm64-v8a/libsnbk_android.so`
//! is the library an APK loads; the game data has to be in the app's files
//! directory, because `test_games/` is not part of the APK.

#[path = "snbk.rs"]
mod app;

#[path = "android/storage_access.rs"]
mod storage_access;
