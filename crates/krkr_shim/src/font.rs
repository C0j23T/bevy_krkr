//! System font discovery, fallback and bidirectional shaping for host drawing.
use std::sync::OnceLock;

static FALLBACK: OnceLock<&'static [u8]> = OnceLock::new();

pub fn set_fallback_font(bytes: &'static [u8]) {
    let _ = FALLBACK.set(bytes);
}

pub(crate) fn fallback_font() -> &'static [u8] {
    FALLBACK.get().copied().unwrap_or_default()
}

use fontdb::{Database, Family, Query, Style, Weight};
use std::collections::BTreeSet;
use unicode_script::{Script, UnicodeScript};

fn system_fonts() -> &'static Database {
    static SYSTEM: OnceLock<Database> = OnceLock::new();
    SYSTEM.get_or_init(|| {
        let mut database = Database::new();
        database.load_system_fonts();
        #[cfg(target_os = "android")]
        database.load_fonts_dir("/system/fonts");
        database
    })
}

pub fn families() -> BTreeSet<&'static str> {
    system_fonts()
        .faces()
        .flat_map(|face| face.families.iter().map(|(name, _)| name.as_str()))
        .collect()
}

pub fn with_system_font<T>(
    family: &str,
    style: u32,
    callback: impl FnOnce(&[u8], u32) -> T,
) -> Option<T> {
    let database = system_fonts();
    let family = Family::Name(family);
    let query = Query {
        families: &[family],
        weight: if style & 1 != 0 {
            Weight::BOLD
        } else {
            Weight::NORMAL
        },
        style: if style & 2 != 0 {
            Style::Italic
        } else {
            Style::Normal
        },
        ..Default::default()
    };
    database.with_face_data(database.query(&query)?, callback)
}

#[repr(C)]
#[derive(Debug)]
pub struct PositionedGlyph {
    pub glyph: u32,
    pub x_offset: i32,
    pub y_offset: i32,
    pub advance: i32,
    pub cluster: u32,
}

pub fn with_default_font<T>(
    family: &str,
    style: u32,
    callback: impl FnOnce(&[u8], u32) -> T,
) -> Option<T> {
    let mut callback = Some(callback);
    if let Some(result) = with_system_font(family, style, |b, i| callback.take().unwrap()(b, i)) {
        return Some(result);
    }
    let database = system_fonts();
    let id = database
        .query(&Query {
            families: &[Family::SansSerif],
            ..Default::default()
        })
        .or_else(|| database.faces().next().map(|f| f.id));
    if let Some(result) =
        id.and_then(|id| database.with_face_data(id, |b, i| callback.take().unwrap()(b, i)))
    {
        return Some(result);
    }
    (!fallback_font().is_empty()).then(|| callback.take().unwrap()(fallback_font(), 0))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Selected {
    Local(usize),
    System(fontdb::ID),
    Fallback,
}

fn covers(face: &ttf_parser::Face<'_>, cluster: &str) -> bool {
    cluster.chars().all(|c| {
        matches!(c,'\u{200c}'|'\u{200d}'|'\u{fe00}'..='\u{fe0f}'|'\u{e0100}'..='\u{e01ef}')
            || c.is_control()
            || face.glyph_index(c).is_some()
    })
}

pub fn shape_runs(
    faces: &[(&[u8], u32)],
    text: &str,
    style: u32,
    mut visit: impl FnMut(&[u8], u32, &[PositionedGlyph]) -> Result<(), String>,
) -> Result<(), String> {
    use unicode_segmentation::UnicodeSegmentation;
    let local: Vec<_> = faces
        .iter()
        .map(|(bytes, i)| ttf_parser::Face::parse(bytes, *i).ok())
        .collect();
    if local.first().and_then(Option::as_ref).is_none() {
        return Err("invalid drawing font".into());
    }
    let mut choices = std::collections::HashMap::new();
    let mut system_order = Vec::new();
    let mut select = |cluster: &str| {
        if let Some(i) = local
            .iter()
            .position(|f| f.as_ref().is_some_and(|f| covers(f, cluster)))
        {
            return Selected::Local(i);
        }
        if let Some(choice) = choices.get(cluster) {
            return *choice;
        }
        let database = system_fonts();
        if system_order.is_empty() {
            system_order.extend(database.faces());
            system_order.sort_by_key(|f| {
                (
                    u8::from((f.style != Style::Normal) != (style & 2 != 0)),
                    f.weight.0.abs_diff(if style & 1 != 0 { 700 } else { 400 }),
                )
            });
        }
        let selected = system_order
            .iter()
            .find(|f| {
                database.with_face_data(f.id, |bytes, i| {
                    ttf_parser::Face::parse(bytes, i).is_ok_and(|face| covers(&face, cluster))
                }) == Some(true)
            })
            .map(|f| Selected::System(f.id))
            .unwrap_or_else(|| {
                if ttf_parser::Face::parse(fallback_font(), 0).is_ok_and(|f| covers(&f, cluster)) {
                    Selected::Fallback
                } else {
                    Selected::Local(0)
                }
            });
        choices.insert(cluster.to_owned(), selected);
        selected
    };
    let bidi = unicode_bidi::BidiInfo::new(text, None);
    for paragraph in &bidi.paragraphs {
        let (levels, runs) = bidi.visual_runs(paragraph, paragraph.range.clone());
        for run in runs {
            let rtl = levels[run.start].is_rtl();
            let mut spans = Vec::new();
            let mut start = run.start;
            let mut face = None;
            let mut script = None;
            for (offset, cluster) in text[run.clone()].grapheme_indices(true) {
                let next_face = select(cluster);
                let next_script = cluster
                    .chars()
                    .map(|c| c.script())
                    .find(|s| !matches!(s, Script::Inherited | Script::Common));
                if face.is_some()
                    && (face != Some(next_face)
                        || matches!((script,next_script),(Some(a),Some(b)) if a!=b))
                {
                    spans.push((start..run.start + offset, face.unwrap()));
                    start = run.start + offset;
                    script = None;
                }
                face = Some(next_face);
                script = next_script.or(script);
            }
            if let Some(face) = face {
                spans.push((start..run.end, face));
            }
            if rtl {
                spans.reverse();
            }
            for (range, selected) in spans {
                let mut shape = |bytes: &[u8], index: u32| {
                    let font = harfrust::FontRef::from_index(bytes, index)
                        .map_err(|_| "invalid shaping font")?;
                    let data = harfrust::ShaperData::new(&font);
                    let shaper = data.shaper(&font).build();
                    let mut buffer = harfrust::UnicodeBuffer::new();
                    buffer.push_str(&text[range.clone()]);
                    buffer.set_direction(if rtl {
                        harfrust::Direction::RightToLeft
                    } else {
                        harfrust::Direction::LeftToRight
                    });
                    buffer.guess_segment_properties();
                    let shaped = shaper.shape(buffer, Default::default());
                    let glyphs = shaped
                        .glyph_infos()
                        .iter()
                        .zip(shaped.glyph_positions())
                        .map(|(g, p)| PositionedGlyph {
                            glyph: g.glyph_id,
                            x_offset: p.x_offset,
                            y_offset: p.y_offset,
                            advance: p.x_advance,
                            cluster: range.start as u32 + g.cluster,
                        })
                        .collect::<Vec<_>>();
                    visit(bytes, index, &glyphs)
                };
                match selected {
                    Selected::Local(i) => shape(faces[i].0, faces[i].1)?,
                    Selected::System(id) => system_fonts()
                        .with_face_data(id, shape)
                        .ok_or("font is unavailable")??,
                    Selected::Fallback => shape(fallback_font(), 0)?,
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const LATIN: &[u8] =
        include_bytes!("../../../tests/fixtures/layer_ex_draw/layer-test-latin.ttf");
    const FALLBACK: &[u8] =
        include_bytes!("../../../tests/fixtures/layer_ex_draw/layer-test-fallback.ttf");

    #[test]
    fn shaping_uses_the_primary_face_and_keeps_graphemes_together_in_fallback() {
        let mut runs = Vec::new();
        shape_runs(
            &[(LATIN, 0), (FALLBACK, 0)],
            "A A\u{301}",
            0,
            |face, _, glyphs| {
                runs.push((
                    face == LATIN,
                    glyphs
                        .iter()
                        .map(|g| (g.glyph, g.cluster, g.advance))
                        .collect::<Vec<_>>(),
                ));
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(
            runs,
            [
                (true, vec![(1, 0, 600), (2, 1, 300)]),
                (false, vec![(1, 2, 500), (2, 2, 0)])
            ]
        );
    }

    #[test]
    fn shaping_emits_bidirectional_runs_in_visual_order() {
        let mut glyphs = Vec::new();
        shape_runs(&[(LATIN, 0), (FALLBACK, 0)], "A אב", 0, |_, _, run| {
            glyphs.extend(run.iter().map(|g| (g.cluster, g.advance)));
            Ok(())
        })
        .unwrap();
        assert_eq!(glyphs, [(0, 600), (1, 300), (4, 700), (2, 650)]);
    }
}
