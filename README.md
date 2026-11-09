# bevy_krkr

Kirikiri2 / KAG3 on [Bevy](https://bevyengine.org) 0.20, reusing
[Kirikiroid2](https://github.com/zeas2/Kirikiroid2) for everything it already
implements.

TJS2, XP3 storage and layer compositing run in Kirikiroid2's C++ code. Rust
reaches them through FFI bindings that are **generated at build time**. The
Rust code covers ECS integration, texture upload, the sound device the engine
mixes onto, and resource lifecycle. There is no `.ks` parser and no TJS2
parser in this repository.

## Layout

```
KirikiriEnterEvent ─▶ handle_enter ─▶ krkr_engine_init, Tjs2Engine, host.tjs
Update:     Tjs2Set (krkr_engine_tick)
PostUpdate: sync_screen (composited RGBA8 → Image → Sprite on KirikiriCamera), poll_engine_exit
Last:       KirikiriExitEvent / AppExit ─▶ teardown (screen → TJS2 → archive → engine)
```

## Kirikiroid2 source

`krkr_sys` compiles the Kirikiroid2 engine core itself (about 160 C/C++
files, with `cc`). It looks for the checkout in this order:

1. `$KIRIKIROID2_SRC`, if set (an error if it is not a directory);
2. `Kirikiroid2/` in the workspace root.

The checkout must be on the **`bevy-port`** branch: it adds
`src/core/environ/bevy/` (platform layer replacing cocos2d-x, compat headers)
and a few marked fixes, described in its `README.md`. A checkout without it
stops the build with an explanation.

It is a git submodule of this repository
([C0j23T/Kirikiroid2](https://github.com/C0j23T/Kirikiroid2), branch
`bevy-port`, a fork of [zeas2/Kirikiroid2](https://github.com/zeas2/Kirikiroid2)):

```sh
git clone --recursive https://github.com/C0j23T/bevy_krkr
# or, in an existing clone:
git submodule update --init
```

System requirements (found through pkg-config): a C++17 compiler, libclang
(bindgen), oniguruma, freetype2, zlib, libpng, libjpeg-turbo, libwebp,
vorbisfile, opusfile, tinyxml2 and OpenCV 4 or 5 (core/imgproc). On Arch:
`pacman -S oniguruma freetype2 libpng libjpeg-turbo libwebp libvorbis opusfile
tinyxml2 opencv clang`.

| Variable | Meaning |
|---|---|
| `KIRIKIROID2_SRC` | Kirikiroid2 checkout (default `./Kirikiroid2`). |
| `KRKR_CODEGEN_VERBOSE=1` | Log codegen steps. |
| `KRKR_RESOURCE_DIR` | Extra directories (`:`-separated) searched for the fallback font `DroidSansFallback.ttf` (default: system font dirs). |

## Building

```sh
cargo build                          # real bindings + Kirikiroid2 core (feature real_ffi, default)
cargo build --no-default-features    # stub bindings, no Kirikiroid2 needed
```

The first build compiles the engine core (about 15 s on 16 cores); it is
rebuilt only when files under `Kirikiroid2/src` or `codegen/` change. The core
is always built with `-O2`.

Stub mode (no feature, or no checkout found at the default path, which prints
a warning) gives `krkr_sys` functions with the same signatures that panic
with their name. Everything compiles and the unit tests run.

## Codegen

Pipeline, in `crates/krkr_sys/build.rs` (modelled on bevy_mod_scripting's
generate → collect → expand):

1. **generate**: the C ABI is declared once, as `FnDecl` tables in
   `codegen/src/api/*.rs`. Each entry has its C signature, the Rust
   signature bindgen produces, and the C++ statements that forward it to
   Kirikiroid2 (`TVPExecuteExpression`, `tTVPLayerManager` draw buffer, …).
2. **collect**: the copyright headers of the Kirikiroid2 sources the shim
   calls into are collected.
3. **expand** into `$OUT_DIR`:
   - `krkr_c_api.h`: `extern "C"` declarations, collected notices on top;
   - `krkr_c_api.cpp`: notices, `templates/shim_prelude.cpp`, then every
     body wrapped as `try { … } KRKR_SHIM_CATCH(failure)`. C++ exceptions
     never cross into Rust; the message is kept for `krkr_last_error()`.
4. bindgen turns the header into `bindings.rs` (included by
   `crates/krkr_sys/src/lib.rs`); `cc` compiles the sources listed in
   `codegen/src/port.rs` plus the shim; pkg-config links the system libraries.

In stub mode, step 4 is replaced by `krkr_codegen::generate_rust_stubs()`,
expanded from the same tables, so stub and real signatures cannot drift apart.

Inspect the output without cargo:

```sh
cargo run -p krkr_codegen -- --src Kirikiroid2 --out-header /tmp/krkr_c_api.h --out-cpp /tmp/krkr_c_api.cpp --verbose
```

## Adding a binding

1. Add an `FnDecl` to the table of its subsystem in `codegen/src/api/`
   (or a new module listed in `api::GROUPS`): name, doc, C and Rust
   signatures, `cpp_target`, and `cpp_body`. Throw on failure; the generator
   adds the exception guard and the failure return value. Shared helpers go
   in `templates/shim_prelude.cpp`.
2. If it needs Kirikiroid2 code that is not compiled yet, add it to
   `codegen/src/port.rs`.
3. Wrap it safely in the crate that owns the concept (`krkr_script`,
   `krkr_plugin`). `krkr_sys` stays raw and Bevy-free.
4. `cargo test -p krkr_codegen` checks that every function is in the header,
   shim and stubs and that the Rust types follow bindgen's mapping;
   `cargo build` checks the real build.

## Using it

```rust
use bevy::prelude::*;
use bevy_krkr::prelude::*;

App::new()
    .add_plugins((DefaultPlugins, BevyKrkrPlugin::default()))
    .add_systems(Startup, |mut commands: Commands, mut enter: MessageWriter<KirikiriEnterEvent>| {
        commands.spawn((Camera2d, KirikiriCamera)); // you own the camera
        enter.write(KirikiriEnterEvent { data_xp3_path: "data.xp3".into() });
    })
    .run();
```

- `KirikiriExitEvent` is written when the game ends itself — `System.exit`,
  `System.terminate`, or `Window.close()` on the main window. It stops the
  *instance* and the Bevy app keeps running, so another `KirikiriEnterEvent`
  starts a game again. Write the message yourself to stop an instance. Ending
  the app is `AppExit`, a separate decision the plugin never makes for you (it
  does tear the instance down first).
- The plugin starts the engine and runs `host.tjs`; the game's own scripts run
  when the host calls `Tjs2Engine::exec_startup` (see `examples/snbk.rs`).

## Example

```sh
cargo run --example snbk                     # test_games/snbk/data.xp3
cargo run --example snbk -- path/to/data.xp3 --dump-log engine.log
```

`snbk.rs` starts a retail game archive: it mounts every archive next to the
data file (that game keeps its voice/CG/event images in **7z** archives) and
runs the game's own start-up scripts. Its `startup.tjs` builds the game UI on a
Kirikiri `Window`, which the port renders off-screen and shows.

That archive's `startup.tjs` currently stops at `PSBFile` (see below), so it
does not reach the game's own title screen yet; the example prints where it
stopped.

## Status

- Working: engine start/stop, TJS2, text layout and rendering by Kirikiroid2
  (Layer, Font, FreeType), off-screen compositing uploaded as a texture,
  BGM/SE/voice channels, XP3/TAR/7z/ZIP reading, the compiled-in plugins, TJS
  `Timer` (and so a game's `[wait time=...]`, transitions and auto/skip
  timing), engine log → Bevy log.
- Input: Bevy's keyboard, mouse and touches are forwarded to the game's own
  `Window` as the TJS `onKeyDown` / `onKeyUp` / `onMouseMove` / `onMouseDown` /
  `onMouseUp` calls a native window manager would produce, with keys translated
  to Win32 virtual-key codes and positions to Kirikiri pixels
  (`crates/krkr_plugin/src/input.rs`,
  `Kirikiroid2/src/core/environ/bevy/BevyWindowLayer.cpp`).
- `Window`: a game that builds its own UI gets a `Window` that renders
  off-screen into a texture the host shows in place of the script-created
  screen, with mouse/key events posted back to its TJS handlers
  (`Kirikiroid2/src/core/environ/bevy/BevyWindowLayer.cpp`).
- Not taken from Kirikiroid2: input and window management (the Bevy host owns
  them), the sound device (cpal, through the mix the engine produces), video,
  BPG/JXR/PVR images. Scripts using them get a "not supported by the
  bevy_krkr port" error.
- Tested on Linux x86-64 (GCC 16) only.

## License

GPL-2.0-or-later (`LICENSE`). See `LICENSE-THIRD-PARTY.md` for Kirikiroid2's
license and the system libraries the engine core links. bevy_krkr uses Kirikiroid2
code, copyright W.Dee and contributors, and is not affiliated with or endorsed
by the Kirikiroid2 authors.
