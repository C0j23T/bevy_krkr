//! The KAG execution cursor and the systems that move it.
//!
//! The cursor is a plain [`Resource`]; it is advanced by messages
//! ([`KagAdvance`], [`KagJumpRequest`]) and by the wait timer, never stored on
//! an entity. Each frame, while the cursor is [`CursorState::Running`],
//! [`kag_step`] asks the C++ parser for tags and turns each one into Bevy
//! messages through [`interpret`] until a tag pauses the cursor.

use std::collections::VecDeque;
use std::time::Duration;

use bevy::input::keyboard::KeyCode;
use bevy::input::mouse::MouseButton;
use bevy::input::ButtonInput;
use bevy::prelude::*;
use krkr_script::Tjs2Engine;

use crate::events::*;
use crate::parser::{KagParser, RawTag, Step};

/// Upper bound on tags processed per frame, so a scenario without waits
/// cannot stall a frame.
pub const MAX_TAGS_PER_FRAME: usize = 256;

#[derive(Debug, Clone, Default, PartialEq)]
pub enum CursorState {
    /// No scenario is loaded.
    #[default]
    Idle,
    Running,
    /// Paused at `[l]` / `[p]` until [`KagAdvance`].
    WaitClick,
    /// Paused at `[wait]`.
    WaitTime {
        can_skip: bool,
    },
    /// Paused at `[s]` until [`KagJumpRequest`].
    Stopped,
    /// The scenario ended or the parser failed.
    Ended,
}

/// Position and pause state of the running scenario.
#[derive(Resource, Debug, Default)]
pub struct KagCursor {
    pub state: CursorState,
    /// Scenario passed to [`KagCursor::start`].
    pub storage: Option<String>,
    /// Message-window text since the last clear.
    pub text: String,
    /// Length of `text` already sent in a [`ShowMessageEvent`].
    shown: usize,
    /// Set by `[p]`: clear the text when advancing.
    clear_on_advance: bool,
    timer: Option<Timer>,
    /// Tags returned by the parser but not yet interpreted because an earlier
    /// tag in the same step paused the cursor.
    pending: VecDeque<RawTag>,
}

impl KagCursor {
    /// Reset for a newly loaded scenario.
    pub fn start(&mut self, storage: &str) {
        *self = Self {
            state: CursorState::Running,
            storage: Some(storage.to_owned()),
            ..default()
        };
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    fn flush_text(&mut self, out: &mut Vec<KagOutput>) {
        if self.text.len() > self.shown {
            self.shown = self.text.len();
            out.push(KagOutput::ShowMessage(ShowMessageEvent {
                text: self.text.clone(),
                speaker: None,
            }));
        }
    }

    fn clear_text(&mut self, out: &mut Vec<KagOutput>) {
        self.text.clear();
        self.shown = 0;
        out.push(KagOutput::ClearMessage(ClearMessageEvent));
    }
}

/// One message produced by [`interpret`].
#[derive(Debug, Clone, PartialEq)]
pub enum KagOutput {
    ShowSprite(ShowSpriteEvent),
    HideSprite(HideSpriteEvent),
    FadeLayer(FadeLayerEvent),
    PlayBgm(PlayBgmEvent),
    StopBgm(StopBgmEvent),
    PauseBgm(PauseBgmEvent),
    ResumeBgm(ResumeBgmEvent),
    BgmVolume(BgmVolumeEvent),
    PlaySe(PlaySeEvent),
    StopSe(StopSeEvent),
    PlayVoice(PlayVoiceEvent),
    StopVoice(StopVoiceEvent),
    ShowMessage(ShowMessageEvent),
    ClearMessage(ClearMessageEvent),
    Wait(WaitEvent),
    WaitClick(WaitClickEvent),
    Jump(JumpEvent),
    Eval(EvalEvent),
    Quake(QuakeEvent),
    Unhandled(UnhandledTagEvent),
}

impl KagOutput {
    fn write(self, commands: &mut Commands) {
        match self {
            Self::ShowSprite(m) => commands.write_message(m),
            Self::HideSprite(m) => commands.write_message(m),
            Self::FadeLayer(m) => commands.write_message(m),
            Self::PlayBgm(m) => commands.write_message(m),
            Self::StopBgm(m) => commands.write_message(m),
            Self::PauseBgm(m) => commands.write_message(m),
            Self::ResumeBgm(m) => commands.write_message(m),
            Self::BgmVolume(m) => commands.write_message(m),
            Self::PlaySe(m) => commands.write_message(m),
            Self::StopSe(m) => commands.write_message(m),
            Self::PlayVoice(m) => commands.write_message(m),
            Self::StopVoice(m) => commands.write_message(m),
            Self::ShowMessage(m) => commands.write_message(m),
            Self::ClearMessage(m) => commands.write_message(m),
            Self::Wait(m) => commands.write_message(m),
            Self::WaitClick(m) => commands.write_message(m),
            Self::Jump(m) => commands.write_message(m),
            Self::Eval(m) => commands.write_message(m),
            Self::Quake(m) => commands.write_message(m),
            Self::Unhandled(m) => commands.write_message(m),
        };
    }
}

// ── attribute helpers (KAG attribute values are strings) ────────────────────

struct Attrs<'a>(&'a serde_json::Map<String, serde_json::Value>);

impl Attrs<'_> {
    fn str(&self, key: &str) -> Option<String> {
        match self.0.get(key)? {
            serde_json::Value::String(s) => Some(s.clone()),
            serde_json::Value::Null => None,
            other => Some(other.to_string()),
        }
    }
    fn num<T: std::str::FromStr>(&self, key: &str) -> Option<T> {
        self.str(key)?.trim().parse().ok()
    }
    fn bool(&self, key: &str, default: bool) -> bool {
        match self
            .str(key)
            .as_deref()
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("true" | "1" | "yes") => true,
            Some("false" | "0" | "no") => false,
            _ => default,
        }
    }
    /// KAG volume 0–100 → 0.0–1.0.
    fn volume(&self, key: &str) -> f32 {
        self.num::<f32>(key)
            .map_or(1.0, |v| (v / 100.0).clamp(0.0, 1.0))
    }
    /// `base` → 0, `n` → n + 1. `None` for message layers or garbage.
    fn layer(&self) -> Option<u32> {
        match self.str("layer").as_deref().unwrap_or("base") {
            "base" => Some(0),
            n => n.parse::<u32>().ok().map(|n| n + 1),
        }
    }
}

/// Translate one tag reported by Kirikiroid2 into Bevy messages, updating the
/// cursor's text buffer and pause state. Tags are not parsed here; the tag
/// name and attributes come from the C++ parser.
pub fn interpret(tag: &RawTag, cursor: &mut KagCursor) -> Vec<KagOutput> {
    let a = Attrs(&tag.attrs);
    let mut out = Vec::new();
    let unhandled = || {
        KagOutput::Unhandled(UnhandledTagEvent {
            name: tag.name.clone(),
            attrs: tag.attrs.clone(),
        })
    };

    match tag.name.as_str() {
        // message window
        "ch" => cursor.text.push_str(&a.str("text").unwrap_or_default()),
        "r" => cursor.text.push('\n'),
        "l" | "p" => {
            let page_break = tag.name == "p";
            cursor.flush_text(&mut out);
            cursor.clear_on_advance = page_break;
            cursor.state = CursorState::WaitClick;
            out.push(KagOutput::WaitClick(WaitClickEvent { page_break }));
        }
        "cm" | "er" | "ct" => cursor.clear_text(&mut out),
        "s" => {
            cursor.flush_text(&mut out);
            cursor.state = CursorState::Stopped;
        }
        "wait" => {
            let ms = a.num::<u32>("time").unwrap_or(0);
            let can_skip = a.bool("canskip", true);
            cursor.flush_text(&mut out);
            cursor.timer = Some(Timer::new(
                Duration::from_millis(ms.into()),
                TimerMode::Once,
            ));
            cursor.state = CursorState::WaitTime { can_skip };
            out.push(KagOutput::Wait(WaitEvent {
                milliseconds: ms,
                can_skip,
            }));
        }
        "jump" => out.push(KagOutput::Jump(JumpEvent {
            storage: a.str("storage"),
            label: a.str("target"),
        })),
        "eval" => match a.str("exp") {
            Some(expression) => out.push(KagOutput::Eval(EvalEvent { expression })),
            None => out.push(unhandled()),
        },

        // layers
        "image" => match a.layer() {
            Some(layer) => out.push(KagOutput::ShowSprite(ShowSpriteEvent {
                layer,
                storage: a.str("storage").unwrap_or_default(),
                back_page: a.str("page").as_deref() == Some("back"),
                left: a.num("left").unwrap_or(0),
                top: a.num("top").unwrap_or(0),
                visible: a.bool("visible", true),
                opacity: a
                    .num::<f32>("opacity")
                    .map_or(1.0, |o| (o / 255.0).clamp(0.0, 1.0)),
            })),
            None => out.push(unhandled()),
        },
        "freeimage" => match a.layer() {
            Some(layer) => out.push(KagOutput::HideSprite(HideSpriteEvent { layer })),
            None => out.push(unhandled()),
        },
        "layopt" => match (a.layer(), a.num::<f32>("opacity")) {
            (Some(layer), Some(o)) => out.push(KagOutput::FadeLayer(FadeLayerEvent {
                layer,
                target_alpha: (o / 255.0).clamp(0.0, 1.0),
                duration_ms: 0,
            })),
            _ => out.push(unhandled()),
        },

        // BGM
        "playbgm" | "fadeinbgm" => out.push(KagOutput::PlayBgm(PlayBgmEvent {
            storage: a.str("storage").unwrap_or_default(),
            looping: a.bool("loop", true),
            volume: 1.0,
            fade_ms: if tag.name == "fadeinbgm" {
                a.num("time").unwrap_or(0)
            } else {
                0
            },
        })),
        "stopbgm" | "fadeoutbgm" => out.push(KagOutput::StopBgm(StopBgmEvent {
            fade_ms: if tag.name == "fadeoutbgm" {
                a.num("time").unwrap_or(0)
            } else {
                0
            },
        })),
        "pausebgm" => out.push(KagOutput::PauseBgm(PauseBgmEvent)),
        "resumebgm" => out.push(KagOutput::ResumeBgm(ResumeBgmEvent)),
        "bgmopt" | "fadebgm" => out.push(KagOutput::BgmVolume(BgmVolumeEvent {
            volume: a.volume("volume"),
            fade_ms: a.num("time").unwrap_or(0),
        })),

        // SE / voice
        "playse" => out.push(KagOutput::PlaySe(PlaySeEvent {
            storage: a.str("storage").unwrap_or_default(),
            buffer: a.num("buf").unwrap_or(0),
            volume: 1.0,
            looping: a.bool("loop", false),
        })),
        "stopse" => out.push(KagOutput::StopSe(StopSeEvent {
            buffer: a.num("buf").unwrap_or(0),
        })),
        "playvoice" | "voice" => out.push(KagOutput::PlayVoice(PlayVoiceEvent {
            storage: a.str("storage").unwrap_or_default(),
            volume: a.volume("volume"),
        })),
        "stopvoice" => out.push(KagOutput::StopVoice(StopVoiceEvent)),

        // effects
        "quake" => out.push(KagOutput::Quake(QuakeEvent {
            amplitude_x: a.num("hmax").unwrap_or(10.0),
            amplitude_y: a.num("vmax").unwrap_or(10.0),
            duration_ms: a.num("time").unwrap_or(0),
        })),

        _ => out.push(unhandled()),
    }
    out
}

// ── systems ─────────────────────────────────────────────────────────────────

/// Sends [`KagAdvance`] on left click, Enter or Space while the cursor waits.
pub fn kag_input(
    cursor: Res<KagCursor>,
    mouse: Option<Res<ButtonInput<MouseButton>>>,
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mut advance: MessageWriter<KagAdvance>,
) {
    let waiting = matches!(
        cursor.state,
        CursorState::WaitClick | CursorState::WaitTime { can_skip: true }
    );
    let clicked = mouse.is_some_and(|m| m.just_pressed(MouseButton::Left))
        || keys.is_some_and(|k| k.any_just_pressed([KeyCode::Enter, KeyCode::Space]));
    if waiting && clicked {
        advance.write(KagAdvance);
    }
}

/// Applies [`KagJumpRequest`]s to the parser and resumes the cursor.
pub fn kag_jump_requests(
    mut requests: MessageReader<KagJumpRequest>,
    mut cursor: ResMut<KagCursor>,
    parser: Option<NonSendMut<KagParser>>,
) {
    let Some(mut parser) = parser else {
        requests.clear();
        return;
    };
    for req in requests.read() {
        match parser.jump(req.storage.as_deref(), req.label.as_deref()) {
            Ok(()) => {
                cursor.pending.clear();
                cursor.timer = None;
                cursor.state = CursorState::Running;
            }
            Err(e) => warn!("kag: jump to {:?}/{:?} failed: {e}", req.storage, req.label),
        }
    }
}

/// Resumes the cursor on [`KagAdvance`] and ticks `[wait]` timers.
pub fn kag_advance(
    mut advance: MessageReader<KagAdvance>,
    mut cursor: ResMut<KagCursor>,
    time: Res<Time>,
    mut commands: Commands,
) {
    let advanced = advance.read().count() > 0;
    match cursor.state.clone() {
        CursorState::WaitClick if advanced => {
            if std::mem::take(&mut cursor.clear_on_advance) {
                let mut out = Vec::new();
                cursor.clear_text(&mut out);
                out.into_iter().for_each(|o| o.write(&mut commands));
            }
            cursor.state = CursorState::Running;
        }
        CursorState::WaitTime { can_skip } => {
            let finished = cursor
                .timer
                .as_mut()
                .is_none_or(|t| t.tick(time.delta()).is_finished());
            if finished || (can_skip && advanced) {
                cursor.timer = None;
                cursor.state = CursorState::Running;
            }
        }
        _ => {}
    }
}

/// Pulls tags from Kirikiroid2's parser and emits messages until the cursor
/// pauses, the scenario ends, or [`MAX_TAGS_PER_FRAME`] is reached.
pub fn kag_step(
    mut cursor: ResMut<KagCursor>,
    parser: Option<NonSendMut<KagParser>>,
    engine: Option<NonSend<Tjs2Engine>>,
    mut commands: Commands,
) {
    let Some(mut parser) = parser else { return };
    let mut processed = 0;
    while cursor.state == CursorState::Running && processed < MAX_TAGS_PER_FRAME {
        let tag = match cursor.pending.pop_front() {
            Some(tag) => tag,
            None => match parser.step() {
                Ok(Step::Tags(tags)) => {
                    cursor.pending.extend(tags);
                    processed += 1;
                    continue;
                }
                Ok(Step::End) => {
                    let mut out = Vec::new();
                    cursor.flush_text(&mut out);
                    out.into_iter().for_each(|o| o.write(&mut commands));
                    cursor.state = CursorState::Ended;
                    commands.write_message(KagScriptEndEvent);
                    break;
                }
                Err(e) => {
                    error!("kag: {e}");
                    cursor.state = CursorState::Ended;
                    commands.write_message(KagScriptEndEvent);
                    break;
                }
            },
        };
        processed += 1;
        for output in interpret(&tag, &mut cursor) {
            match &output {
                KagOutput::Jump(j) => {
                    if let Err(e) = parser.jump(j.storage.as_deref(), j.label.as_deref()) {
                        warn!("kag: [jump] failed: {e}");
                    }
                    cursor.pending.clear();
                }
                KagOutput::Eval(ev) => match engine.as_deref() {
                    Some(engine) => {
                        if let Err(e) = engine.eval(&ev.expression) {
                            warn!("kag: [eval exp={:?}] failed: {e}", ev.expression);
                        }
                    }
                    None => warn!("kag: [eval] without a TJS2 engine"),
                },
                _ => {}
            }
            output.write(&mut commands);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(tags: &[RawTag]) -> (KagCursor, Vec<KagOutput>) {
        let mut cursor = KagCursor::default();
        cursor.start("first.ks");
        let out = tags
            .iter()
            .flat_map(|t| interpret(t, &mut cursor))
            .collect();
        (cursor, out)
    }

    #[test]
    fn text_is_flushed_at_l_and_cursor_waits() {
        let (cursor, out) = run(&[
            RawTag::new("ch", &[("text", "Hello")]),
            RawTag::new("l", &[]),
        ]);
        assert_eq!(cursor.state, CursorState::WaitClick);
        assert_eq!(
            out,
            vec![
                KagOutput::ShowMessage(ShowMessageEvent {
                    text: "Hello".into(),
                    speaker: None
                }),
                KagOutput::WaitClick(WaitClickEvent { page_break: false }),
            ]
        );
    }

    #[test]
    fn playbgm_uses_kag_attribute_names() {
        let (_, out) = run(&[RawTag::new(
            "fadeinbgm",
            &[("storage", "bgm.ogg"), ("loop", "false"), ("time", "500")],
        )]);
        assert_eq!(
            out,
            vec![KagOutput::PlayBgm(PlayBgmEvent {
                storage: "bgm.ogg".into(),
                looping: false,
                volume: 1.0,
                fade_ms: 500
            })]
        );
    }

    #[test]
    fn image_layer_mapping() {
        let (_, out) = run(&[
            RawTag::new("image", &[("storage", "bg"), ("layer", "base")]),
            RawTag::new(
                "image",
                &[
                    ("storage", "ch"),
                    ("layer", "0"),
                    ("left", "100"),
                    ("opacity", "255"),
                ],
            ),
            RawTag::new("image", &[("storage", "x"), ("layer", "message0")]),
        ]);
        assert!(matches!(&out[0], KagOutput::ShowSprite(e) if e.layer == 0));
        assert!(
            matches!(&out[1], KagOutput::ShowSprite(e) if e.layer == 1 && e.left == 100 && e.opacity == 1.0)
        );
        assert!(matches!(&out[2], KagOutput::Unhandled(e) if e.name == "image"));
    }

    #[test]
    fn wait_and_stop_pause_the_cursor() {
        let (cursor, out) = run(&[RawTag::new(
            "wait",
            &[("time", "250"), ("canskip", "false")],
        )]);
        assert_eq!(cursor.state, CursorState::WaitTime { can_skip: false });
        assert_eq!(
            out,
            vec![KagOutput::Wait(WaitEvent {
                milliseconds: 250,
                can_skip: false
            })]
        );
        let (cursor, _) = run(&[RawTag::new("s", &[])]);
        assert_eq!(cursor.state, CursorState::Stopped);
    }

    #[test]
    fn unknown_tags_are_forwarded() {
        let (_, out) = run(&[RawTag::new("mymacro", &[("x", "1")])]);
        assert!(
            matches!(&out[0], KagOutput::Unhandled(e) if e.name == "mymacro" && e.attrs["x"] == "1")
        );
    }

    #[test]
    fn page_break_clears_on_advance() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<KagAdvance>()
            .add_message::<ClearMessageEvent>();
        let mut cursor = KagCursor::default();
        cursor.start("first.ks");
        for t in [RawTag::new("ch", &[("text", "a")]), RawTag::new("p", &[])] {
            interpret(&t, &mut cursor);
        }
        app.insert_resource(cursor).add_systems(Update, kag_advance);
        app.world_mut().write_message(KagAdvance);
        app.update();
        let cursor = app.world().resource::<KagCursor>();
        assert_eq!(cursor.state, CursorState::Running);
        assert!(cursor.text.is_empty());
        assert_eq!(
            app.world().resource::<Messages<ClearMessageEvent>>().len(),
            1
        );
    }
}
