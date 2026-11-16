# ffmpeg-sys-next patch

Vendored from `ffmpeg-sys-next` 9.1.1 (MIT, upstream repository:
https://github.com/zmwangx/rust-ffmpeg). The local Cargo patch keeps the
upstream source and applies two Android NDK build fixes in `build.rs`:

- Resolve `llvm-nm` and `llvm-strip` beside the compiler using its parent
  directory.
- Pass cargo-ndk's API-qualified `--target` flag to bindgen, as required by
  NDK 30 headers.
- Build against FFmpeg release/9.0 on Android. `ffmpeg-next` 9.0.0 is the
  Rust wrapper selected by this workspace, and FFmpeg 9.1 adds enum values
  that wrapper does not yet handle exhaustively.
