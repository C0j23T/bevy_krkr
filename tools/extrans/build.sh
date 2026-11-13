#!/usr/bin/env bash
set -euo pipefail
source_dir=${1:-$HOME/Workspace/extrans}
stub_dir=${2:-$HOME/Workspace/krkrz/tp_stub}
output=${3:-$source_dir/extrans.so}
script_dir=$(cd -- "$(dirname -- "$0")" && pwd)
"${CXX:-c++}" -std=c++17 -O2 -shared -fPIC -fvisibility=hidden -fvisibility-inlines-hidden \
  -D__GENERIC__ -I"$stub_dir" -I"$script_dir" -I"$source_dir" \
  "$source_dir"/{Main,wave,mosaic,turn,turntrans_table,rotatebase,rotatetrans,ripple}.cpp \
  "$stub_dir/tp_stub.cpp" "$script_dir/v2link.cpp" -o "$output"
