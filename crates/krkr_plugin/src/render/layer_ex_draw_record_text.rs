use super::{MAX_POINTS, Object, Parser, Reader, Region, rectangle, region::elements, unit_pixels};
use parley::{
    FontContext, FontFamilyName, FontStyle, FontWeight, Layout, LayoutContext,
    PositionedLayoutItem, StyleProperty, TextWrapMode,
};
use std::cell::RefCell;
use ttf_parser::{Face, GlyphId, OutlineBuilder};
use vello_gpu::kurbo::{Affine, BezPath, Point, Rect, Shape};

impl Reader<'_> {
    fn utf16(&mut self, count: usize) -> Result<String, String> {
        let bytes = self.take(count * 2)?;
        Ok(char::decode_utf16(
            bytes
                .chunks_exact(2)
                .map(|b| u16::from_le_bytes([b[0], b[1]])),
        )
        .map(|r| r.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect())
    }
}

#[derive(Clone)]
pub(super) struct Font {
    family: String,
    size: f64,
    unit: u32,
    style: u32,
}

impl Font {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, String> {
        r.version()?;
        let size = r.f32()?;
        let unit = r.u32()?;
        let style = r.u32()?;
        r.take(4)?;
        let len = r.count(32768)?;
        let family = r.utf16(len)?.trim_end_matches('\0').to_owned();
        if size <= 0. || unit > 6 || style & !15 != 0 {
            return Err("invalid EMF+ font".into());
        }
        Ok(Self {
            family,
            size,
            unit,
            style,
        })
    }
}

#[derive(Clone)]
pub(super) struct StringFormat {
    flags: u32,
    align: u32,
    line_align: u32,
    margins: [f64; 2],
    tracking: f64,
    hotkey: u32,
}

impl Default for StringFormat {
    fn default() -> Self {
        Self {
            flags: 0,
            align: 0,
            line_align: 0,
            margins: [1. / 6.; 2],
            tracking: 1.,
            hotkey: 0,
        }
    }
}

impl StringFormat {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, String> {
        r.version()?;
        let flags = r.u32()?;
        r.take(4)?;
        let align = r.u32()?;
        let line_align = r.u32()?;
        let _digits = r.u32()?;
        r.take(4)?;
        let _first_tab = r.f32()?;
        let hotkey = r.u32()?;
        let margins = r.floats()?;
        let tracking = r.f32()?;
        let trimming = r.u32()?;
        let tabs = r.count(65536)?;
        let ranges = r.count(65536)?;
        r.take(tabs * 4 + ranges * 8)?;
        if align > 2 || line_align > 2 || hotkey > 2 || tracking <= 0. || trimming > 5 {
            return Err("invalid EMF+ string format".into());
        }
        Ok(Self {
            flags,
            align,
            line_align,
            margins,
            tracking,
            hotkey,
        })
    }
}

struct GlyphOutline<'a> {
    path: &'a mut BezPath,
    matrix: Affine,
}

impl GlyphOutline<'_> {
    fn point(&self, x: f32, y: f32) -> Point {
        self.matrix * Point::new(x.into(), y.into())
    }
}

impl OutlineBuilder for GlyphOutline<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.path.move_to(self.point(x, y));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.path.line_to(self.point(x, y));
    }
    fn quad_to(&mut self, a: f32, b: f32, x: f32, y: f32) {
        self.path.quad_to(self.point(a, b), self.point(x, y));
    }
    fn curve_to(&mut self, a: f32, b: f32, c: f32, d: f32, x: f32, y: f32) {
        self.path
            .curve_to(self.point(a, b), self.point(c, d), self.point(x, y));
    }
    fn close(&mut self) {
        self.path.close_path();
    }
}

fn glyph(
    path: &mut BezPath,
    face: &Face<'_>,
    id: GlyphId,
    size: f64,
    x: f64,
    y: f64,
) -> Result<(), String> {
    let scale = size / f64::from(face.units_per_em());
    let matrix = Affine::new([scale, 0., 0., -scale, x, y]);
    face.outline_glyph(id, &mut GlyphOutline { path, matrix });
    if path.elements().len() > MAX_POINTS {
        return Err("EMF+ glyph outline limit".into());
    }
    Ok(())
}

thread_local! {
    static LAYOUT: RefCell<(FontContext,LayoutContext<()>)> = RefCell::new((FontContext::new(),LayoutContext::new()));
}

fn text_path(
    source: &str,
    font: &Font,
    size: f64,
    format: &StringFormat,
    rect: [f64; 4],
) -> Result<BezPath, String> {
    let [x, y, w, h] = rect;
    let vertical = format.flags & 2 != 0;
    let (w, h) = if vertical { (h, w) } else { (w, h) };
    let [leading, trailing] = format.margins.map(|n| n * size);
    let available = (w - leading - trailing).max(0.);
    let rtl = format.flags & 1 != 0;
    let mut text = String::new();
    let mut hotkeys = Vec::new();
    if rtl {
        text.push('\u{202b}');
    }
    let mut chars = source.chars();
    while let Some(c) = chars.next() {
        if c == '&' && format.hotkey != 0 {
            if let Some(c) = chars.next() {
                let start = text.len();
                text.push(c);
                if c != '&' && format.hotkey == 1 {
                    hotkeys.push(start..text.len());
                }
            }
        } else {
            text.push(c);
        }
    }
    if rtl {
        text.push('\u{202c}');
    }
    let layout: Layout<()> = LAYOUT.with_borrow_mut(|(fonts, layouts)| {
        let mut b = layouts.ranged_builder(fonts, &text, 1., false);
        b.push_default(FontFamilyName::named(&font.family));
        b.push_default(StyleProperty::FontSize(size as f32));
        if font.style & 1 != 0 {
            b.push_default(StyleProperty::FontWeight(FontWeight::BOLD));
        }
        if font.style & 2 != 0 {
            b.push_default(StyleProperty::FontStyle(FontStyle::Italic));
        }
        if format.flags & 0x1000 != 0 {
            b.push_default(StyleProperty::TextWrapMode(TextWrapMode::NoWrap));
        }
        for range in hotkeys {
            b.push(StyleProperty::Underline(true), range);
        }
        b.build(&text)
    });
    let mut layout = layout;
    layout.break_all_lines((w > 0.).then_some((available / format.tracking) as f32));
    let align = |slack: f64, mode| match mode {
        1 => slack / 2.,
        2 => slack,
        _ => 0.,
    };
    let top = align(
        if h > 0. {
            h - f64::from(layout.height())
        } else {
            0.
        },
        format.line_align,
    );
    let mut path = BezPath::new();
    for line in layout.lines() {
        let m = line.metrics();
        if h > 0. && format.flags & 0x2000 != 0 && f64::from(m.block_max_coord) + top > h {
            break;
        }
        let advance = if format.flags & 0x800 != 0 {
            m.advance
        } else {
            m.advance - m.trailing_whitespace
        };
        let align_mode = if rtl { 2 - format.align } else { format.align };
        let left = leading
            + align(
                if w > 0. {
                    available - f64::from(advance) * format.tracking
                } else {
                    -f64::from(advance) * format.tracking
                },
                align_mode,
            );
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(run) = item else {
                continue;
            };
            let f = run.run().font();
            let face =
                Face::parse(f.data.as_ref(), f.index).map_err(|_| "invalid EMF+ layout font")?;
            for g in run.positioned_glyphs() {
                glyph(
                    &mut path,
                    &face,
                    GlyphId(g.id as u16),
                    size,
                    left + f64::from(g.x) * format.tracking,
                    top + f64::from(g.y),
                )?;
            }
            let metrics = run.run().metrics();
            for (active, offset, thickness) in [
                (
                    font.style & 4 != 0 || run.style().underline.is_some(),
                    metrics.underline_offset,
                    metrics.underline_size,
                ),
                (
                    font.style & 8 != 0,
                    metrics.strikethrough_offset,
                    metrics.strikethrough_size,
                ),
            ] {
                if active {
                    let x = left + f64::from(run.offset()) * format.tracking;
                    let y = top + f64::from(run.baseline() - offset);
                    path.extend(
                        Rect::new(
                            x,
                            y,
                            x + f64::from(run.advance()) * format.tracking,
                            y + f64::from(thickness),
                        )
                        .to_path(0.1),
                    );
                }
            }
        }
    }
    let transform = if vertical {
        Affine::new([0., 1., -1., 0., x + h, y])
    } else {
        Affine::translate((x, y))
    };
    Ok(transform * path)
}

impl Parser<'_> {
    pub(super) fn text(&mut self, kind: u16, flags: u16, r: &mut Reader<'_>) -> Result<(), String> {
        let paint = self.brush(flags, r.u32()?)?;
        let Some(Object::Font(font)) = self.objects.get(&(flags as u8)) else {
            return Err("missing EMF+ font".into());
        };
        let size = if font.unit == 0 {
            font.size
        } else {
            font.size * unit_pixels(font.unit, self.logical_dpi)?[1]
                / unit_pixels(self.state.page_unit, self.logical_dpi)?[1]
        };
        let mut clip = None;
        let path = if kind == 0x401c {
            let id = r.u32()?;
            let default = StringFormat::default();
            let format = if id == u32::MAX {
                &default
            } else {
                match self.objects.get(&(id as u8)).filter(|_| id < 64) {
                    Some(Object::StringFormat(f)) => f,
                    _ => return Err("missing EMF+ string format".into()),
                }
            };
            let length = r.count(1 << 20)?;
            let rect = r.floats()?;
            if rect[2] > 0. && rect[3] > 0. && format.flags & 0x4000 == 0 {
                clip = Some(Region::from_path(&rectangle(rect), 0).transform(self.state.matrix()));
            }
            text_path(&r.utf16(length)?, font, size, format, rect)?
        } else {
            let options = r.u32()?;
            let matrix_present = r.u32()?;
            let count = r.count(1 << 20)?;
            if options & !15 != 0 || matrix_present > 1 {
                return Err("invalid EMF+ driver string".into());
            }
            let codes = (0..count).map(|_| r.u16()).collect::<Result<Vec<_>, _>>()?;
            let realized = options & 4 != 0;
            let positions = (0..if realized { count.min(1) } else { count })
                .map(|_| r.floats::<2>())
                .collect::<Result<Vec<_>, _>>()?;
            let matrix = if matrix_present != 0 {
                Affine::new(r.floats()?)
            } else {
                Affine::IDENTITY
            };
            let path =
                krkr_shim::font::with_default_font(&font.family, font.style, |bytes, index| {
                    let face = Face::parse(bytes, index).map_err(|_| "invalid EMF+ driver font")?;
                    let mut path = BezPath::new();
                    let mut pos = positions.first().copied().unwrap_or_default();
                    for (i, c) in codes.into_iter().enumerate() {
                        if !realized {
                            pos = positions[i];
                        }
                        let id = if options & 1 == 0 {
                            GlyphId(c)
                        } else {
                            char::from_u32(c.into())
                                .and_then(|ch| face.glyph_index(ch))
                                .unwrap_or(GlyphId(0))
                        };
                        glyph(&mut path, &face, id, size, pos[0], pos[1])?;
                        let vertical = options & 2 != 0;
                        let advance = if vertical {
                            face.glyph_ver_advance(id)
                        } else {
                            face.glyph_hor_advance(id)
                        };
                        pos[usize::from(vertical)] += size
                            * f64::from(advance.unwrap_or(face.units_per_em()))
                            / f64::from(face.units_per_em());
                    }
                    Ok::<_, String>(matrix * path)
                })
                .ok_or("EMF+ driver font is unavailable")??;
            path
        };
        let previous = self.state.region.clone();
        if let Some(clip) = clip {
            self.state.region = previous.combine(&clip, 1)?;
        }
        let result = self.emit(elements(&path), 1, paint);
        self.state.region = previous;
        result
    }
}
