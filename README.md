# bevy_krkr

Kirikiri2 / KAG3 on [Bevy](https://bevyengine.org) 0.20, reusing
[Kirikiroid2](https://github.com/zeas2/Kirikiroid2) for everything it already
implements.

TJS2, the KAG3 parser, XP3 storage and layer compositing run in Kirikiroid2's
C++ code. Rust reaches them through FFI bindings that are **generated at
build time**. The Rust code covers ECS integration, texture upload, audio
scheduling through `bevy_kira_audio`, and resource lifecycle. There is no
`.ks` parser and no TJS2 parser in this repository.

## Layout

```
bevy_krkr/
├── build.rs              workspace-level env check (codegen runs in krkr_sys/build.rs)
├── codegen/              krkr_codegen
│   ├── src/api.rs        the C ABI surface: types, constants, callbacks
│   ├── src/api/*.rs      one table of FnDecl per subsystem (engine, tjs, kag, xp3, layer)
│   ├── src/port.rs       Kirikiroid2 sources / include dirs / flags / system libs to build
│   └── templates/shim_prelude.cpp   support code shared by the generated shim
├── crates/
│   ├── krkr_sys/         raw FFI; src/lib.rs only include!()s the generated file
│   ├── krkr_script/      Tjs2Engine (non-send resource), Tjs2Plugin
│   ├── krkr_kag/         KagParser, KagCursor, KAG tag → Bevy messages
│   ├── krkr_audio/       BGM / SE / voice channels on bevy_kira_audio
│   └── krkr_plugin/      BevyKrkrPlugin, lifecycle, host.tjs + message window, KrkrRenderPlugin
├── src/lib.rs            re-exports + prelude
├── examples/minimal_kag.rs
├── assets/game/          first.ks + bgm.wav for the example
└── Kirikiroid2/          submodule: C0j23T/Kirikiroid2, branch bevy-port
```

```
KirikiriEnterEvent ─▶ handle_enter ─▶ krkr_engine_init, Tjs2Engine, host.tjs, KagParser
Update:     Tjs2Set (krkr_engine_tick) ─▶ KagSet (kag_step: KAGParser → tags → messages) ─▶ KrkrAudioSet
PostUpdate: message_window (ShowMessageEvent → krkrBevy.showMessage in TJS2)
            ─▶ sync_screen (composited RGBA8 → Image → Sprite on KirikiriCamera), poll_engine_exit
Last:       KirikiriExitEvent / AppExit ─▶ teardown (parser → screen → TJS2 → archive → engine)
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
vorbisfile, opusfile, lz4, xxhash, tinyxml2 and OpenCV 4 or 5
(core/imgproc). On Arch: `pacman -S oniguruma freetype2 libpng libjpeg-turbo
libwebp libvorbis opusfile lz4 xxhash tinyxml2 opencv clang`.

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
   Kirikiroid2 (`KAGParser.getNextTag`, `TVPExecuteExpression`,
   `tTVPLayerManager` draw buffer, …).
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
   `krkr_kag`, `krkr_plugin`). `krkr_sys` stays raw and Bevy-free.
4. `cargo test -p krkr_codegen` checks that every function is in the header,
   shim and stubs and that the Rust types follow bindgen's mapping;
   `cargo build` checks the real build.

## Using it

```rust
use bevy::prelude::*;
use bevy_krkr::prelude::*;

App::new()
    .add_plugins((DefaultPlugins, BevyKrkrPlugin::default())) // start scenario: first.ks
    .add_systems(Startup, |mut commands: Commands, mut enter: MessageWriter<KirikiriEnterEvent>| {
        commands.spawn((Camera2d, KirikiriCamera)); // you own the camera
        enter.write(KirikiriEnterEvent { data_xp3_path: "data.xp3".into() });
    })
    .run();
```

- Bevy 0.20 calls buffered events *messages*. The KAG outputs keep their
  `…Event` names but are `Message`s: read them with `MessageReader`.
- `KirikiriExitEvent` is written when script code terminates the engine;
  write it yourself to stop an instance. `AppExit` also tears the instance
  down.
- `KagCursor` (a resource) holds the scenario position and pause state. It
  advances on `KagAdvance` (left click / Enter / Space by default) and
  `KagJumpRequest`.
- Tags without a built-in mapping arrive as `UnhandledTagEvent`.
- Audio storage names resolve through `KrkrAudioSettings`
  (`storage_root` + name + `default_extension`).

## Example

```sh
cargo run --example minimal_kag              # uses assets/game
cargo run --example minimal_kag -- data.xp3
```

`assets/game/first.ks` starts `bgm.wav` (looping, through `bevy_kira_audio`)
and shows "Hello from Kirikiri on Bevy." in the message window. Click (or
Enter / Space) shows the second line; a second click ends the scenario, stops
the BGM and closes the window.

## Status

- Working: engine start/stop, TJS2, KAGParser driving the KAG cursor, text
  layout and rendering by Kirikiroid2 (Layer, Font, FreeType), off-screen
  compositing uploaded as a texture, BGM/SE/voice channels, XP3/TAR reading,
  engine log → Bevy log.
- One instance at a time, and the same process can start another one after an
  exit. Stopping an instance returns its memory. Kirikiroid2 is written for a
  process that ends with the engine, so the port does what that assumption
  covered: `krkr_engine_shutdown` clears the process-global state (TJS2
  interpreter and its script blocks, event queue, storage caches, font
  rasterisers) and `krkr_engine_init` resets the one-shot initialisation
  guards. The teardown releases the host objects before the interpreter goes
  away, because TJS2 has no cycle collector and the screen's layer tree would
  otherwise keep its bitmaps. `tests/restart_memory.rs` measures it.
- Not taken from Kirikiroid2: its Window class and input (the Bevy host owns
  them), sound output (bevy_kira_audio), video, BPG/JXR/PVR images and
  ZIP/7z archives. Scripts using them get an "not supported by the bevy_krkr
  port" error.
- The message window is the minimal one in `crates/krkr_plugin/src/host.tjs`;
  KAG3's `MessageLayer.tjs` system scripts are not used. Speaker names, ruby
  and per-character effects are not implemented.
- Audio files are loaded from Bevy's asset folder. Reading them out of
  `data.xp3` needs an `AssetReader` over `KrkrSession::read_file`.
- Tested on Linux x86-64 (GCC 16) only.

## License

GPL-2.0-or-later (`LICENSE`). See `LICENSE-THIRD-PARTY.md` for Kirikiroid2's
license and the system libraries the engine core links. bevy_krkr uses Kirikiroid2
code, copyright W.Dee and contributors, and is not affiliated with or endorsed
by the Kirikiroid2 authors.
