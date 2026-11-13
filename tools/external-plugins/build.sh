#!/usr/bin/env bash
set -euo pipefail
plugin=${1:-all}
workspace=${2:-$HOME/Workspace}
stub_dir=${3:-$workspace/krkrz/tp_stub}
script_dir=$(cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(cd -- "$script_dir/../.." && pwd)
build_dir=$repo_dir/target/external-plugins
ncbind_dir=${4:-$build_dir/ncbind}
mkdir -p "$build_dir"
if [[ $plugin != extNagano && ! -f $ncbind_dir/ncbind.cpp ]]; then
  git clone https://github.com/wamsoft/ncbind.git "$ncbind_dir"
  git -C "$ncbind_dir" checkout --detach 256502b817e6627c1491b916938448470df395fd
fi
flags=(-std=c++17 -O2 -shared -fPIC -fvisibility=hidden -fvisibility-inlines-hidden -D__GENERIC__ -I"$stub_dir")
if [[ $plugin == all || $plugin == extNagano ]]; then
  source_dir=$workspace/extNagano
  "${CXX:-c++}" "${flags[@]}" -I"$script_dir/../extrans" -I"$source_dir" \
    "$source_dir"/{Main,3duniversal,blurfade,scanline,zoomfade,rgbfade,spin,flutter,book,imagewipe,honeyturn,morphing,multiripple}.cpp \
    "$stub_dir/tp_stub.cpp" "$script_dir/../extrans/v2link.cpp" -o "$source_dir/extNagano.so"
fi
if [[ $plugin == all || $plugin == AlphaMovie ]]; then
  source_dir=$workspace/AlphaMovie
  "${CXX:-c++}" "${flags[@]}" -DTVP_PLUGIN_NAME=AlphaMovie -I"$ncbind_dir" \
    "$source_dir/AlphaMovie.cpp" "$ncbind_dir/ncbind.cpp" "$stub_dir/tp_stub.cpp" \
    -lz -o "$source_dir/AlphaMovie.so"
fi
if [[ $plugin == all || $plugin == psdfile ]]; then
  source_dir=$workspace/psdfile
  if git -C "$source_dir" apply --check "$script_dir/psdfile-bmp-header.patch" 2>/dev/null; then
    git -C "$source_dir" apply "$script_dir/psdfile-bmp-header.patch"
  elif ! git -C "$source_dir" apply --reverse --check "$script_dir/psdfile-bmp-header.patch" 2>/dev/null; then
    echo "psdclass.cpp does not match the BMP header compatibility patch" >&2
    exit 1
  fi
  cmake -S "$source_dir" -B "$build_dir/psdfile" -DTPSTUB_DIR="$stub_dir" \
    -DNCBIND_DIR="$ncbind_dir" -DCMAKE_POSITION_INDEPENDENT_CODE=ON \
    -DCMAKE_BUILD_TYPE=Release -DKRKRZ_VARIANT=LIB
  cmake --build "$build_dir/psdfile" --parallel 8
  cp "$build_dir/psdfile/libpsdfile.so" "$source_dir/psdfile.so"
fi
