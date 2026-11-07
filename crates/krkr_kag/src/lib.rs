//! `krkr_kag` – KAG3 tags from Kirikiroid2 → Bevy messages.
//!
//! ```text
//! KAGParser (C++) ──tag callback──▶ parser::KagParser::step
//!                                          │
//!                       systems::kag_step ─┴─ systems::interpret
//!                                          │
//!          ShowSpriteEvent, PlayBgmEvent, ShowMessageEvent, WaitClickEvent, …
//! ```
//!
//! The parser itself is created by `krkr_plugin` when Kirikiri starts. This
//! crate registers the messages, the [`KagCursor`] resource and the systems;
//! the systems do nothing until a [`KagParser`] non-send resource exists.

pub mod events;
pub mod parser;
pub mod systems;

use bevy::prelude::*;

pub use events::*;
pub use parser::{KagError, KagParser, RawTag};
pub use systems::{interpret, CursorState, KagCursor, KagOutput};

/// Marks the [`Camera2d`] that shows Kirikiri output. Add it to exactly one
/// camera you spawn yourself; bevy_krkr does not spawn cameras.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct KirikiriCamera;

/// System set for KAG work in `Update`. Runs after `krkr_script::Tjs2Set`.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct KagSet;

#[derive(Default)]
pub struct KagPlugin;

impl Plugin for KagPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ShowSpriteEvent>()
            .add_message::<HideSpriteEvent>()
            .add_message::<FadeLayerEvent>()
            .add_message::<PlayBgmEvent>()
            .add_message::<StopBgmEvent>()
            .add_message::<PauseBgmEvent>()
            .add_message::<ResumeBgmEvent>()
            .add_message::<BgmVolumeEvent>()
            .add_message::<PlaySeEvent>()
            .add_message::<StopSeEvent>()
            .add_message::<PlayVoiceEvent>()
            .add_message::<StopVoiceEvent>()
            .add_message::<ShowMessageEvent>()
            .add_message::<ClearMessageEvent>()
            .add_message::<WaitEvent>()
            .add_message::<WaitClickEvent>()
            .add_message::<JumpEvent>()
            .add_message::<EvalEvent>()
            .add_message::<QuakeEvent>()
            .add_message::<UnhandledTagEvent>()
            .add_message::<KagScriptLoadedEvent>()
            .add_message::<KagScriptEndEvent>()
            .add_message::<KagAdvance>()
            .add_message::<KagJumpRequest>()
            .init_resource::<KagCursor>()
            .configure_sets(Update, KagSet.after(krkr_script::Tjs2Set))
            .add_systems(
                Update,
                (
                    systems::kag_input,
                    systems::kag_jump_requests,
                    systems::kag_advance,
                    systems::kag_step,
                )
                    .chain()
                    .in_set(KagSet),
            );
    }
}
