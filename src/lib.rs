//! `bevy_krkr` – Kirikiri2 on Bevy, through Kirikiroid2.
//!
//! TJS2, XP3 storage and layer compositing all run in Kirikiroid2 (C++),
//! reached through bindings that `crates/krkr_sys/build.rs` generates at
//! compile time. This crate re-exports the Bevy side.
//!
//! ```no_run
//! use bevy::prelude::*;
//! use bevy_krkr::prelude::*;
//!
//! fn main() {
//!     App::new()
//!         .add_plugins((DefaultPlugins, BevyKrkrPlugin::default()))
//!         .add_systems(Startup, setup)
//!         .run();
//! }
//!
//! fn setup(mut commands: Commands, mut enter: MessageWriter<KirikiriEnterEvent>) {
//!     commands.spawn((Camera2d, KirikiriCamera));
//!     enter.write(KirikiriEnterEvent { data_xp3_path: "data.xp3".into() });
//! }
//! ```

pub use krkr_plugin as plugin;
pub use krkr_script as script;
pub use krkr_sys as sys;

pub use krkr_plugin::BevyKrkrPlugin;

pub mod prelude {
    pub use krkr_plugin::input::{shift_state, virtual_key, ScreenPlacement};
    pub use krkr_plugin::render::{KirikiriScreen, KirikiriScreenImage, KIRIKIRI_RENDER_LAYER};
    pub use krkr_plugin::{
        BevyKrkrPlugin, KirikiriCamera, KirikiriEnterEvent, KirikiriExitEvent, KirikiriState,
    };
}
