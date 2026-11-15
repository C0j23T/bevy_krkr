#!/usr/bin/env bash
# Cross-builds the system libraries krkr_sys links when building for Android:
# freetype2, oniguruma, vorbisfile (ogg + vorbis) and opusfile (opus + ogg).
#
#   tools/android/build-deps.sh [--abi arm64-v8a] [--api 26]
#
# Needs curl, tar, CMake, Make and the NDK (`ANDROID_NDK_HOME` or
# `ANDROID_NDK_ROOT`, or the newest NDK under `$ANDROID_HOME/ndk`). Sources are
# cached in `target/android-deps/src`, the prefix is
# `target/android-deps/$abi`. Point pkg-config at it when building bevy_krkr:
#
#   export PKG_CONFIG_LIBDIR=$PWD/target/android-deps/arm64-v8a/lib/pkgconfig
#   export PKG_CONFIG_ALLOW_CROSS=1

set -euo pipefail

root=$(cd "$(dirname "$0")/../.." && pwd)
abi=arm64-v8a
api=${KRKR_ANDROID_API:-26}

while [ $# -gt 0 ]; do
    case $1 in
        --abi) abi=$2; shift 2 ;;
        --api) api=$2; shift 2 ;;
        -h|--help) sed -n '2,14p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
done

case $abi in
    arm64-v8a) triple=aarch64-linux-android ;;
    armeabi-v7a) triple=armv7a-linux-androideabi ;;
    x86) triple=i686-linux-android ;;
    x86_64) triple=x86_64-linux-android ;;
    *) echo "unsupported ABI: $abi" >&2; exit 2 ;;
esac

ndk=${ANDROID_NDK_HOME:-${ANDROID_NDK_ROOT:-}}
if [ -z "$ndk" ] && [ -n "${ANDROID_HOME:-}" ] && [ -d "$ANDROID_HOME/ndk" ]; then
    ndk=$ANDROID_HOME/ndk/$(ls "$ANDROID_HOME/ndk" | sort -V | tail -1)
fi
if [ ! -f "$ndk/build/cmake/android.toolchain.cmake" ]; then
    echo "set ANDROID_NDK_HOME to an NDK (looked at: ${ndk:-<unset>})" >&2
    exit 1
fi

prefix=${KRKR_ANDROID_DEPS:-$root/target/android-deps}/$abi
cache=$root/target/android-deps/src
source_dir=$root/target/android-deps/build/$abi/src
build_dir=$root/target/android-deps/build/$abi/build
mkdir -p "$prefix" "$cache" "$source_dir" "$build_dir"

llvm=
for candidate in "$ndk"/toolchains/llvm/prebuilt/*; do
    llvm=$candidate
    break
done
bin=$llvm/bin

fetch() {
    local file=$1 sha=$2
    shift 2
    if [ ! -f "$cache/$file" ]; then
        local url
        for url in "$@"; do
            echo "downloading $file from $url"
            if curl -fL --retry 3 --connect-timeout 20 -o "$cache/$file.part" "$url"; then
                mv "$cache/$file.part" "$cache/$file"
                break
            fi
        done
        if [ ! -f "$cache/$file" ]; then
            echo "cannot download $file" >&2
            exit 1
        fi
    fi
    echo "$sha  $cache/$file" | sha256sum -c --quiet
}

extract() {
    local name=$1 file=$2
    if [ ! -d "$source_dir/$name" ]; then
        mkdir -p "$source_dir/$name"
        tar -xf "$cache/$file" -C "$source_dir/$name" --strip-components=1
    fi
}

build_cmake() {
    local name=$1 pc=$2
    shift 2
    if [ -f "$prefix/lib/pkgconfig/$pc" ]; then
        echo "== $name: already built"
        return
    fi
    echo "== $name"
    cmake -S "$source_dir/$name" -B "$build_dir/$name" \
        -DCMAKE_POLICY_VERSION_MINIMUM=3.5 \
        -DCMAKE_TOOLCHAIN_FILE="$ndk/build/cmake/android.toolchain.cmake" \
        -DANDROID_ABI="$abi" \
        -DANDROID_PLATFORM="android-$api" \
        -DCMAKE_BUILD_TYPE=Release \
        -DCMAKE_POSITION_INDEPENDENT_CODE=ON \
        -DCMAKE_INSTALL_PREFIX="$prefix" \
        -DBUILD_SHARED_LIBS=OFF \
        "$@"
    cmake --build "$build_dir/$name" --parallel
    cmake --install "$build_dir/$name"
}

fetch freetype-2.13.3.tar.xz \
    0550350666d427c74daeb85d5ac7bb353acba5f76956395995311a9c6f063289 \
    https://download.savannah.gnu.org/releases/freetype/freetype-2.13.3.tar.xz \
    https://downloads.sourceforge.net/freetype/freetype2/2.13.3/freetype-2.13.3.tar.xz
fetch oniguruma-6.9.10.tar.gz \
    25302fed5198600bad82cc47106afff74148dabea02b2d2de6b26137ce247d12 \
    https://gitee.com/mirrors/oniguruma/repository/archive/v6.9.10.tar.gz
fetch libogg-1.3.5.tar.xz \
    c4d91be36fc8e54deae7575241e03f4211eb102afb3fc0775fbbc1b740016705 \
    https://downloads.xiph.org/releases/ogg/libogg-1.3.5.tar.xz \
    https://ftp.osuosl.org/pub/xiph/releases/ogg/libogg-1.3.5.tar.xz
fetch libvorbis-1.3.7.tar.xz \
    b33cc4934322bcbf6efcbacf49e3ca01aadbea4114ec9589d1b1e9d20f72954b \
    https://downloads.xiph.org/releases/vorbis/libvorbis-1.3.7.tar.xz \
    https://ftp.osuosl.org/pub/xiph/releases/vorbis/libvorbis-1.3.7.tar.xz
fetch opus-1.5.2.tar.gz \
    65c1d2f78b9f2fb20082c38cbe47c951ad5839345876e46941612ee87f9a7ce1 \
    https://downloads.xiph.org/releases/opus/opus-1.5.2.tar.gz \
    https://ftp.osuosl.org/pub/xiph/releases/opus/opus-1.5.2.tar.gz
fetch opusfile-0.12.tar.gz \
    118d8601c12dd6a44f52423e68ca9083cc9f2bfe72da7a8c1acb22a80ae3550b \
    https://downloads.xiph.org/releases/opus/opusfile-0.12.tar.gz \
    https://ftp.osuosl.org/pub/xiph/releases/opus/opusfile-0.12.tar.gz

extract freetype freetype-2.13.3.tar.xz
extract oniguruma oniguruma-6.9.10.tar.gz
extract ogg libogg-1.3.5.tar.xz
extract vorbis libvorbis-1.3.7.tar.xz
extract opus opus-1.5.2.tar.gz
extract opusfile opusfile-0.12.tar.gz

build_cmake freetype freetype2.pc \
    -DFT_DISABLE_ZLIB=TRUE -DFT_DISABLE_BZIP2=TRUE -DFT_DISABLE_PNG=TRUE \
    -DFT_DISABLE_HARFBUZZ=TRUE -DFT_DISABLE_BROTLI=TRUE -DFT_DISABLE_SVG=TRUE
build_cmake oniguruma oniguruma.pc -DINSTALL_DOCUMENTATION=OFF
build_cmake ogg ogg.pc -DBUILD_TESTING=OFF -DINSTALL_DOCS=OFF
build_cmake vorbis vorbisfile.pc \
    -DCMAKE_PREFIX_PATH="$prefix" \
    -DOGG_INCLUDE_DIR="$prefix/include" \
    -DOGG_LIBRARY="$prefix/lib/libogg.a" \
    -DBUILD_TESTING=OFF
build_cmake opus opus.pc \
    -DOPUS_BUILD_TESTING=OFF -DOPUS_BUILD_PROGRAMS=OFF -DOPUS_BUILD_SHARED_LIBRARY=OFF

if [ ! -f "$prefix/lib/pkgconfig/opusfile.pc" ]; then
    echo "== opusfile"
    (
        cd "$source_dir/opusfile"
        env \
            CC="$bin/$triple$api-clang" \
            CXX="$bin/$triple$api-clang++" \
            AR="$bin/llvm-ar" \
            RANLIB="$bin/llvm-ranlib" \
            STRIP="$bin/llvm-strip" \
            PKG_CONFIG_LIBDIR="$prefix/lib/pkgconfig" \
            PKG_CONFIG_ALLOW_CROSS=1 \
            ./configure --host="$triple" --prefix="$prefix" \
                --disable-examples --disable-doc --disable-http
        make -j"$(nproc)"
        make install
    )
else
    echo "== opusfile: already built"
fi

echo
echo "built into $prefix"
echo "  export PKG_CONFIG_LIBDIR=$prefix/lib/pkgconfig"
echo "  export PKG_CONFIG_ALLOW_CROSS=1"
