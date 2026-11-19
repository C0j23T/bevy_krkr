//! Host-supplied fallback font. Static bytes remain valid for every engine session.
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
    pub character: u32,
}

pub fn with_default_font<T>(family:&str, style:u32, callback:impl FnOnce(&[u8],u32)->T) -> Option<T> {
    let mut callback=Some(callback);
    if let Some(result)=with_system_font(family,style,|b,i| callback.take().unwrap()(b,i)) {return Some(result);}
    let database=system_fonts();
    let id=database.query(&Query {families:&[Family::SansSerif],..Default::default()})
        .or_else(||database.faces().next().map(|f|f.id));
    if let Some(result)=id.and_then(|id|database.with_face_data(id,|b,i|callback.take().unwrap()(b,i))) {return Some(result);}
    (!fallback_font().is_empty()).then(||callback.take().unwrap()(fallback_font(),0))
}

pub fn shape_line(bytes: &[u8], index: u32, text: &str) -> Option<Vec<PositionedGlyph>> {
    let font = harfrust::FontRef::from_index(bytes, index).ok()?;
    let data = harfrust::ShaperData::new(&font);
    let shaper = data.shaper(&font).build();
    let mut output = Vec::with_capacity(text.len());
    let bidi = unicode_bidi::BidiInfo::new(text, None);
    for paragraph in &bidi.paragraphs {
        let (levels, runs) = bidi.visual_runs(paragraph, paragraph.range.clone());
        for run in runs {
            let rtl = levels[run.start].is_rtl();
            let mut scripts = Vec::new();
            let mut script = Script::Common;
            let mut start = run.start;
            for (offset, ch) in text[run.clone()].char_indices() {
                let current = ch.script();
                if matches!(current, Script::Inherited | Script::Common) {
                    continue;
                }
                if script != Script::Common && script != current {
                    scripts.push(start..run.start + offset);
                    start = run.start + offset;
                }
                script = current;
            }
            scripts.push(start..run.end);
            if rtl {
                scripts.reverse();
            }
            for range in scripts {
                let source = &text[range];
                let mut buffer = harfrust::UnicodeBuffer::new();
                buffer.push_str(source);
                buffer.set_direction(if rtl {
                    harfrust::Direction::RightToLeft
                } else {
                    harfrust::Direction::LeftToRight
                });
                buffer.guess_segment_properties();
                let shaped = shaper.shape(buffer, Default::default());
                for (glyph, position) in shaped.glyph_infos().iter().zip(shaped.glyph_positions()) {
                    output.push(PositionedGlyph {
                        glyph: glyph.glyph_id,
                        x_offset: position.x_offset,
                        y_offset: position.y_offset,
                        advance: position.x_advance,
                        character: source.get(glyph.cluster as usize..)?.chars().next()? as u32,
                    });
                }
            }
        }
    }
    Some(output)
}
