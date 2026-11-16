#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
video_sdk="${ANDROID_SDK_ROOT:-$HOME/Android/Sdk}"
video_ndk="${ANDROID_NDK_HOME:-$video_sdk/ndk/30.0.16248370}"
video_build_tools="$video_sdk/build-tools/36.0.0"
video_out="$PWD/target/android-video-test"
video_lib="$PWD/target/android-release/aarch64-linux-android/release/examples/libandroid_video.so"
python3 tools/android/extract-video.py
mkdir -p "$video_out/apk/lib/arm64-v8a" "$video_out/apk/assets"
cp "$video_lib" "$video_out/apk/lib/arm64-v8a/"
cp "$video_ndk/toolchains/llvm/prebuilt/linux-x86_64/sysroot/usr/lib/aarch64-linux-android/libc++_shared.so" "$video_out/apk/lib/arm64-v8a/"
cp "$video_out/media/fd_op01.mpg" "$video_out/apk/assets/"
"$video_build_tools/aapt" package -f -M examples/video/AndroidManifest.xml -I "$video_sdk/platforms/android-37.0/android.jar" -A "$video_out/apk/assets" -F "$video_out/unsigned.apk"
(cd "$video_out/apk"; zip -q "$video_out/unsigned.apk" lib/arm64-v8a/*.so)
"$video_build_tools/zipalign" -f -P 16 4 "$video_out/unsigned.apk" "$video_out/aligned.apk"
if [ ! -f "$video_out/debug.jks" ]; then
    keytool -genkeypair -keystore "$video_out/debug.jks" -storepass android -keypass android -alias androiddebugkey -dname 'CN=Android Video Test' -keyalg RSA -validity 3650
fi
"$video_build_tools/apksigner" sign --ks "$video_out/debug.jks" --ks-pass pass:android --out "$video_out/fate-video-arm64.apk" "$video_out/aligned.apk"
"$video_build_tools/apksigner" verify "$video_out/fate-video-arm64.apk"
sha256sum "$video_out/fate-video-arm64.apk" > "$video_out/fate-video-arm64.apk.sha256"
ls -lh "$video_out/fate-video-arm64.apk"
