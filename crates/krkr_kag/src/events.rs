//! Messages produced from KAG3 tags.
//!
//! Bevy 0.20 calls buffered, frame-to-frame events *messages*
//! (`#[derive(Message)]`, `MessageReader`, `MessageWriter`). The `…Event`
//! names are kept because they describe KAG events; they are messages.
//!
//! Attribute names follow KAG3 (`storage`, `layer`, `left`, `top`, `time`,
//! …). Volumes are normalised from KAG's 0–100 to 0.0–1.0.

use bevy::prelude::*;

// ── layers ──────────────────────────────────────────────────────────────────

/// `[image]`: load `storage` into a layer. `layer` 0 is `base`, n+1 is `layer=n`.
#[derive(Message, Debug, Clone, PartialEq)]
pub struct ShowSpriteEvent {
    pub layer: u32,
    pub storage: String,
    /// `true` for `page=back`.
    pub back_page: bool,
    pub left: i32,
    pub top: i32,
    pub visible: bool,
    /// 0.0–1.0, from `opacity` 0–255.
    pub opacity: f32,
}

/// `[freeimage]`.
#[derive(Message, Debug, Clone, PartialEq)]
pub struct HideSpriteEvent {
    pub layer: u32,
}

/// `[layopt opacity=…]` or `[trans]`.
#[derive(Message, Debug, Clone, PartialEq)]
pub struct FadeLayerEvent {
    pub layer: u32,
    pub target_alpha: f32,
    pub duration_ms: u32,
}

// ── BGM ─────────────────────────────────────────────────────────────────────

/// `[playbgm]` / `[fadeinbgm]`.
#[derive(Message, Debug, Clone, PartialEq)]
pub struct PlayBgmEvent {
    pub storage: String,
    pub looping: bool,
    pub volume: f32,
    pub fade_ms: u32,
}

/// `[stopbgm]` / `[fadeoutbgm]`.
#[derive(Message, Debug, Clone, PartialEq)]
pub struct StopBgmEvent {
    pub fade_ms: u32,
}

/// `[pausebgm]`.
#[derive(Message, Debug, Clone, PartialEq)]
pub struct PauseBgmEvent;

/// `[resumebgm]`.
#[derive(Message, Debug, Clone, PartialEq)]
pub struct ResumeBgmEvent;

/// `[bgmopt]` / `[fadebgm]`.
#[derive(Message, Debug, Clone, PartialEq)]
pub struct BgmVolumeEvent {
    pub volume: f32,
    pub fade_ms: u32,
}

// ── SE / voice ──────────────────────────────────────────────────────────────

/// `[playse]`. `buffer` is KAG's SE buffer index.
#[derive(Message, Debug, Clone, PartialEq)]
pub struct PlaySeEvent {
    pub storage: String,
    pub buffer: u8,
    pub volume: f32,
    pub looping: bool,
}

/// `[stopse]`.
#[derive(Message, Debug, Clone, PartialEq)]
pub struct StopSeEvent {
    pub buffer: u8,
}

/// `[playvoice]` / `[voice]` (common macro names; not KAG3 built-ins).
#[derive(Message, Debug, Clone, PartialEq)]
pub struct PlayVoiceEvent {
    pub storage: String,
    pub volume: f32,
}

/// `[stopvoice]`.
#[derive(Message, Debug, Clone, PartialEq)]
pub struct StopVoiceEvent;

// ── message window ──────────────────────────────────────────────────────────

/// Text accumulated since the last clear, sent when the scenario reaches
/// `[l]`, `[p]` or its end. `text` is cumulative within one page.
#[derive(Message, Debug, Clone, PartialEq)]
pub struct ShowMessageEvent {
    pub text: String,
    pub speaker: Option<String>,
}

/// `[cm]` / `[er]` / `[ct]`, or advancing past `[p]`.
#[derive(Message, Debug, Clone, PartialEq)]
pub struct ClearMessageEvent;

// ── flow ────────────────────────────────────────────────────────────────────

/// `[wait]`. The cursor is paused for `milliseconds`.
#[derive(Message, Debug, Clone, PartialEq)]
pub struct WaitEvent {
    pub milliseconds: u32,
    pub can_skip: bool,
}

/// `[l]` / `[p]`. The cursor is paused until a [`KagAdvance`].
#[derive(Message, Debug, Clone, PartialEq)]
pub struct WaitClickEvent {
    pub page_break: bool,
}

/// `[jump]`. Already applied to the parser when this is sent.
#[derive(Message, Debug, Clone, PartialEq)]
pub struct JumpEvent {
    /// `None` = current scenario.
    pub storage: Option<String>,
    pub label: Option<String>,
}

/// `[eval exp=…]`. Already executed by TJS2 when this is sent.
#[derive(Message, Debug, Clone, PartialEq)]
pub struct EvalEvent {
    pub expression: String,
}

/// `[quake]`.
#[derive(Message, Debug, Clone, PartialEq)]
pub struct QuakeEvent {
    pub amplitude_x: f32,
    pub amplitude_y: f32,
    pub duration_ms: u32,
}

/// Any tag without a built-in mapping, so games can handle their own macros.
#[derive(Message, Debug, Clone, PartialEq)]
pub struct UnhandledTagEvent {
    pub name: String,
    pub attrs: serde_json::Map<String, serde_json::Value>,
}

// ── lifecycle ───────────────────────────────────────────────────────────────

#[derive(Message, Debug, Clone, PartialEq)]
pub struct KagScriptLoadedEvent {
    pub storage: String,
}

/// The scenario has no more tags.
#[derive(Message, Debug, Clone, PartialEq)]
pub struct KagScriptEndEvent;

// ── input to the cursor ─────────────────────────────────────────────────────

/// Resume the cursor from `[l]`, `[p]` or a skippable `[wait]`.
/// Sent by the built-in click/key handler, or by game code.
#[derive(Message, Debug, Clone, Default, PartialEq)]
pub struct KagAdvance;

/// Move the parser to `storage`/`label` and resume, also from `[s]`.
#[derive(Message, Debug, Clone, PartialEq)]
pub struct KagJumpRequest {
    pub storage: Option<String>,
    pub label: Option<String>,
}
