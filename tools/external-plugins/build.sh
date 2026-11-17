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
if [[ $plugin != extNagano && $plugin != gpu_fixture && ! -f $ncbind_dir/ncbind.cpp ]]; then
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
  source_dir=$repo_dir/external_plugins/psdfile
  if git -C "$source_dir" apply --check "$script_dir/psdfile-bmp-header.patch" 2>/dev/null; then
    git -C "$source_dir" apply "$script_dir/psdfile-bmp-header.patch"
  elif ! git -C "$source_dir" apply --reverse --check "$script_dir/psdfile-bmp-header.patch" 2>/dev/null; then
    echo "psdclass.cpp does not match the BMP header compatibility patch" >&2
    exit 1
  fi
  cmake -S "$script_dir/psdfile" -B "$build_dir/psdfile-alias" -DTPSTUB_DIR="$stub_dir" \
    -DNCBIND_DIR="$ncbind_dir" -DCMAKE_POSITION_INDEPENDENT_CODE=ON \
    -DCMAKE_LIBRARY_OUTPUT_DIRECTORY="$build_dir/psdfile" \
    -DCMAKE_BUILD_TYPE=Release -DKRKRZ_VARIANT=LIB
  cmake --build "$build_dir/psdfile-alias" --parallel 8
fi

if [[ $plugin == all || $plugin == json ]]; then
  cmake -S "$repo_dir/external_plugins/json" -B "$build_dir/json" \
    -DTPSTUB_DIR="$stub_dir" -DNCBIND_DIR="$ncbind_dir" -DCMAKE_BUILD_TYPE=Release
  cmake --build "$build_dir/json" --parallel 4
fi

if [[ $plugin == all || $plugin == gpu_fixture ]]; then
  cmake -S "$script_dir/gpu-fixture" -B "$build_dir/gpu_fixture" \
    -DTPSTUB_DIR="$stub_dir" -DCMAKE_BUILD_TYPE=Release
  cmake --build "$build_dir/gpu_fixture" --parallel 4
fi

if [[ $plugin == all || $plugin == psbfile ]]; then
  cmake -S "$repo_dir/external_plugins/psbfile" -B "$build_dir/psbfile" \
    -DTPSTUB_DIR="$stub_dir" -DNCBIND_DIR="$ncbind_dir" -DCMAKE_BUILD_TYPE=Release
  cmake --build "$build_dir/psbfile" --parallel 4
fi

for logic_plugin in sqlite3 ExtKAGParser win32dialog; do
  if [[ $plugin == all || $plugin == "$logic_plugin" ]]; then
    cmake -S "$repo_dir/external_plugins/$logic_plugin" -B "$build_dir/$logic_plugin" \
      -DTPSTUB_DIR="$stub_dir" -DNCBIND_DIR="$ncbind_dir" -DCMAKE_BUILD_TYPE=Release
    cmake --build "$build_dir/$logic_plugin" --parallel 4
  fi
done

if [[ $plugin == all || $plugin == textrender ]]; then
  cmake -S "$repo_dir/external_plugins/textrender" -B "$build_dir/textrender" \
    -DTPSTUB_DIR="$stub_dir" -DNCBIND_DIR="$ncbind_dir" -DCMAKE_BUILD_TYPE=Release
  cmake --build "$build_dir/textrender" --parallel 4
fi
