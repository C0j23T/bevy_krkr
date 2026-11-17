use krkr_shim::layer_ex_draw::{BrushPoint, ColorStop, DrawRequest, Paint, PathElement};
use std::collections::HashMap;

pub(super) const IDENTITY: [f64; 6] = [1., 0., 0., 1., 0., 0.];
const VERSION: u32 = 0xdbc01002;
const MAX_BYTES: usize = 256 * 1024 * 1024;
const MAX_POINTS: usize = 1_000_000;

#[derive(Clone, Debug)]
pub(super) struct Bitmap {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Clone, Debug)]
pub(super) struct Record {
    pub width: u32,
    pub height: u32,
    pub bounds: [f64; 4],
    pub background: u32,
    pub operations: Vec<DrawRequest>,
    pub images: HashMap<u64, Bitmap>,
}

impl Record {
    pub fn new(width: u32, height: u32, background: u32) -> Result<Self, String> {
        if width == 0 || height == 0 || width > 65535 || height > 65535 {
            return Err("invalid metafile dimensions".into());
        }
        Ok(Self {
            width,
            height,
            background,
            bounds: [0., 0., width as f64, height as f64],
            operations: Vec::new(),
            images: HashMap::new(),
        })
    }
}

pub(super) fn solid(color: u32) -> Paint {
    Paint {
        brush_type: 0,
        stroke: false,
        colors: [color; 2],
        wrap: 0,
        hatch: 0,
        image: 0,
        coordinates: [0.; 4],
        transform: IDENTITY,
        offset: [0.; 2],
        width: 1.,
        miter_limit: 10.,
        dash_offset: 0.,
        start_cap: 0,
        end_cap: 0,
        dash_cap: 0,
        join: 0,
        alignment: 0,
        stops: vec![],
        dashes: vec![],
        tile: [0.; 4],
        center: [0.; 2],
        focus: [0.; 2],
        brush_points: vec![],
        surround_colors: vec![],
        brush_flags: 0,
        blend_positions: vec![],
        blend_factors: vec![],
        compound: vec![],
        start_arrow: [0.; 4],
        end_arrow: [0.; 4],
    }
}

pub(super) fn multiply(a: [f64; 6], b: [f64; 6]) -> [f64; 6] {
    [
        a[0] * b[0] + a[1] * b[2],
        a[0] * b[1] + a[1] * b[3],
        a[2] * b[0] + a[3] * b[2],
        a[2] * b[1] + a[3] * b[3],
        a[4] * b[0] + a[5] * b[2] + b[4],
        a[4] * b[1] + a[5] * b[3] + b[5],
    ]
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.at.checked_add(n).ok_or("metafile length overflow")?;
        let result = self.bytes.get(self.at..end).ok_or("truncated metafile")?;
        self.at = end;
        Ok(result)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn f32(&mut self) -> Result<f64, String> {
        let value = f32::from_bits(self.u32()?) as f64;
        if !value.is_finite() || value.abs() > 1e9 {
            return Err("invalid metafile coordinate".into());
        }
        Ok(value)
    }
    fn floats<const N: usize>(&mut self) -> Result<[f64; N], String> {
        let mut result = [0.; N];
        for v in &mut result {
            *v = self.f32()?;
        }
        Ok(result)
    }
    fn count(&mut self, maximum: usize) -> Result<usize, String> {
        let n = self.u32()? as usize;
        if n > maximum {
            return Err("metafile resource limit".into());
        }
        Ok(n)
    }
    fn version(&mut self) -> Result<(), String> {
        if self.u32()? >> 12 != 0xdbc01 {
            return Err("invalid EMF+ graphics version".into());
        }
        Ok(())
    }
    fn relative(&mut self) -> Result<f64, String> {
        let a = self.u8()?;
        Ok(if a & 0x80 == 0 {
            ((a << 1) as i8 >> 1) as f64
        } else {
            let b = self.u8()?;
            ((((a as u16 & 0x7f) << 8 | b as u16) << 1) as i16 >> 1) as f64
        })
    }
    fn points(&mut self, n: usize, flags: u16) -> Result<Vec<[f64; 2]>, String> {
        if n > MAX_POINTS {
            return Err("metafile point limit".into());
        }
        let mut result = Vec::with_capacity(n);
        let mut last = [0.; 2];
        for _ in 0..n {
            let p = if flags & 0x800 != 0 {
                [last[0] + self.relative()?, last[1] + self.relative()?]
            } else if flags & 0x4000 != 0 {
                [self.u16()? as i16 as f64, self.u16()? as i16 as f64]
            } else {
                self.floats()?
            };
            result.push(p);
            last = p;
        }
        Ok(result)
    }
}

fn u32w(out: &mut Vec<u8>, value: u32) {
    out.extend(value.to_le_bytes());
}
fn floats(out: &mut Vec<u8>, values: impl IntoIterator<Item = f64>) -> Result<(), String> {
    for value in values {
        if !value.is_finite() || value.abs() > 1e9 {
            return Err("invalid metafile coordinate".into());
        }
        out.extend((value as f32).to_le_bytes());
    }
    Ok(())
}
fn plus(out: &mut Vec<u8>, kind: u16, flags: u16, data: &[u8]) {
    let size = (data.len() + 15) & !3;
    out.extend(kind.to_le_bytes());
    out.extend(flags.to_le_bytes());
    u32w(out, size as u32);
    u32w(out, data.len() as u32);
    out.extend(data);
    out.resize(out.len() + size - 12 - data.len(), 0);
}
fn object(out: &mut Vec<u8>, kind: u16, id: u16, data: &[u8]) {
    plus(out, 0x4008, kind << 8 | id, data);
}

fn path_write(path: &[PathElement], fill: u32) -> Result<Vec<u8>, String> {
    let mut points = Vec::new();
    let mut types: Vec<u8> = Vec::new();
    for element in path {
        match element.verb {
            0 | 1 => {
                floats(&mut points, element.points[..2].iter().copied())?;
                types.push(element.verb as u8);
            }
            2 => {
                floats(&mut points, element.points)?;
                types.extend([3; 3]);
            }
            3 => {
                *types.last_mut().ok_or("close without path")? |= 0x80;
            }
            _ => return Err("invalid path element".into()),
        }
    }
    let mut out = Vec::new();
    u32w(&mut out, VERSION);
    u32w(&mut out, types.len() as u32);
    u32w(&mut out, fill << 13);
    out.extend(points);
    out.extend(types);
    out.resize((out.len() + 3) & !3, 0);
    Ok(out)
}

fn path_read(bytes: &[u8]) -> Result<(Vec<PathElement>, u32), String> {
    let mut r = Reader::new(bytes);
    r.version()?;
    let n = r.count(MAX_POINTS)?;
    let flags = r.u32()?;
    let points = r.points(n, flags as u16)?;
    let mut types = Vec::with_capacity(n);
    if flags & 0x800 != 0 {
        while types.len() < n {
            let run = r.u8()?;
            let kind = r.u8()?;
            let count = (run & 0x3f) as usize;
            if count == 0 || count > n - types.len() {
                return Err("invalid path type run".into());
            }
            types.extend(std::iter::repeat_n(kind, count));
        }
    } else {
        types.extend(r.take(n)?);
    }
    let mut path = Vec::new();
    let mut i = 0;
    let mut open = false;
    while i < n {
        let kind = types[i] & 7;
        let (verb, count) = match kind {
            0 => (0, 1),
            1 => (1, 1),
            3 => (2, 3),
            _ => return Err("invalid path point type".into()),
        };
        if verb != 0 && !open {
            return Err("path has no initial move".into());
        }
        if i + count > n || (count == 3 && types[i..i + 3].iter().any(|v| v & 7 != 3)) {
            return Err("incomplete cubic path".into());
        }
        let mut element = PathElement {
            verb,
            reserved: 0,
            points: [0.; 6],
        };
        for j in 0..count {
            element.points[j * 2..j * 2 + 2].copy_from_slice(&points[i + j]);
        }
        path.push(element);
        open = true;
        if types[i + count - 1] & 0x80 != 0 {
            path.push(PathElement {
                verb: 3,
                reserved: 0,
                points: [0.; 6],
            });
        }
        i += count;
    }
    Ok((path, (flags >> 13) & 1))
}

fn blend_write(out: &mut Vec<u8>, p: &Paint) -> Result<(), String> {
    if !p.stops.is_empty() {
        u32w(out, p.stops.len() as u32);
        floats(out, p.stops.iter().map(|s| s.position))?;
        for s in &p.stops {
            u32w(out, s.color);
        }
    } else if !p.blend_positions.is_empty() {
        u32w(out, p.blend_positions.len() as u32);
        floats(out, p.blend_positions.iter().copied())?;
        floats(out, p.blend_factors.iter().copied())?;
    }
    Ok(())
}
fn blend_read(r: &mut Reader<'_>, flags: u32, p: &mut Paint) -> Result<(), String> {
    if flags & 4 != 0 && flags & 24 != 0 {
        return Err("conflicting gradient blend data".into());
    }
    if flags & 4 != 0 {
        let count = r.count(4096)?;
        let mut positions = Vec::with_capacity(count);
        for _ in 0..count {
            positions.push(r.f32()?);
        }
        for position in positions {
            p.stops.push(ColorStop {
                position,
                color: r.u32()?,
                reserved: 0,
            });
        }
    } else if flags & 24 != 0 {
        if flags & 16 != 0 {
            return Err("vertical gradient blend is unsupported".into());
        }
        let count = r.count(4096)?;
        for _ in 0..count {
            p.blend_positions.push(r.f32()?);
        }
        for _ in 0..count {
            p.blend_factors.push(r.f32()?);
        }
    }
    p.brush_flags = (flags >> 7) & 1;
    Ok(())
}

fn brush_write(p: &Paint, images: &HashMap<u64, Bitmap>) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    u32w(&mut out, VERSION);
    u32w(&mut out, p.brush_type);
    let flags =
        2 | if !p.stops.is_empty() {
            4
        } else if !p.blend_positions.is_empty() {
            8
        } else {
            0
        } | ((p.brush_flags & 1) << 7);
    match p.brush_type {
        0 => u32w(&mut out, p.colors[0]),
        1 => {
            u32w(&mut out, p.hatch);
            for c in p.colors {
                u32w(&mut out, c);
            }
        }
        2 => {
            u32w(&mut out, 2);
            u32w(&mut out, p.wrap);
            floats(&mut out, p.transform)?;
            let image = images
                .get(&p.image)
                .ok_or("metafile texture image is missing")?;
            out.extend(bitmap_write(image)?);
        }
        3 => {
            u32w(&mut out, flags | if p.focus != [0.; 2] { 0x40 } else { 0 });
            u32w(&mut out, p.wrap);
            u32w(&mut out, p.colors[0]);
            floats(&mut out, p.center)?;
            u32w(&mut out, p.surround_colors.len() as u32);
            for c in &p.surround_colors {
                u32w(&mut out, *c);
            }
            u32w(&mut out, p.brush_points.len() as u32);
            for point in &p.brush_points {
                floats(&mut out, [point.x, point.y])?;
            }
            floats(&mut out, p.transform)?;
            blend_write(&mut out, p)?;
            if p.focus != [0.; 2] {
                u32w(&mut out, 2);
                floats(&mut out, p.focus)?;
            }
        }
        4 => {
            u32w(&mut out, flags);
            u32w(&mut out, p.wrap);
            floats(
                &mut out,
                [
                    p.coordinates[0],
                    p.coordinates[1],
                    p.coordinates[2] - p.coordinates[0],
                    p.coordinates[3] - p.coordinates[1],
                ],
            )?;
            for c in p.colors {
                u32w(&mut out, c);
            }
            u32w(&mut out, 0);
            u32w(&mut out, 0);
            floats(&mut out, p.transform)?;
            blend_write(&mut out, p)?;
        }
        _ => return Err("invalid metafile brush".into()),
    }
    Ok(out)
}

fn brush_read(r: &mut Reader<'_>, images: &mut HashMap<u64, Bitmap>) -> Result<Paint, String> {
    r.version()?;
    let mut p = solid(0);
    p.brush_type = r.u32()?;
    match p.brush_type {
        0 => p.colors = [r.u32()?; 2],
        1 => {
            p.hatch = r.u32()?;
            p.colors = [r.u32()?, r.u32()?];
        }
        2 => {
            let flags = r.u32()?;
            p.wrap = r.u32()?;
            if flags & 2 != 0 {
                p.transform = r.floats()?;
            }
            let image = bitmap_read(r)?;
            p.coordinates = [0., 0., image.width as f64, image.height as f64];
            p.image = images.len() as u64 + 1;
            images.insert(p.image, image);
        }
        3 => {
            let flags = r.u32()?;
            p.wrap = r.u32()?;
            p.colors[0] = r.u32()?;
            p.center = r.floats()?;
            let count = r.count(65536)?;
            for _ in 0..count {
                p.surround_colors.push(r.u32()?);
            }
            p.colors[1] = p.surround_colors.first().copied().unwrap_or(p.colors[0]);
            if flags & 1 != 0 {
                let length = r.count(MAX_BYTES)?;
                let (path, _) = path_read(r.take(length)?)?;
                for v in path {
                    if v.verb > 1 {
                        return Err("curved path-gradient boundary is unsupported".into());
                    }
                    p.brush_points.push(BrushPoint {
                        x: v.points[0],
                        y: v.points[1],
                    });
                }
            } else {
                let count = r.count(65536)?;
                for _ in 0..count {
                    let [x, y] = r.floats()?;
                    p.brush_points.push(BrushPoint { x, y });
                }
            }
            if flags & 2 != 0 {
                p.transform = r.floats()?;
            }
            blend_read(r, flags, &mut p)?;
            if flags & 0x40 != 0 {
                if r.u32()? != 2 {
                    return Err("invalid focus scales".into());
                }
                p.focus = r.floats()?;
            }
        }
        4 => {
            let flags = r.u32()?;
            p.wrap = r.u32()?;
            p.tile = r.floats()?;
            p.coordinates = [
                p.tile[0],
                p.tile[1],
                p.tile[0] + p.tile[2],
                p.tile[1] + p.tile[3],
            ];
            p.colors = [r.u32()?, r.u32()?];
            r.take(8)?;
            if flags & 2 != 0 {
                p.transform = r.floats()?;
            }
            blend_read(r, flags, &mut p)?;
        }
        _ => return Err("unsupported EMF+ brush type".into()),
    }
    Ok(p)
}

fn pen_write(p: &Paint, images: &HashMap<u64, Bitmap>) -> Result<Vec<u8>, String> {
    if p.start_cap == 255 || p.end_cap == 255 {
        return Err("EMF+ custom pen caps are not implemented".into());
    }
    let mut out = Vec::new();
    u32w(&mut out, VERSION);
    u32w(&mut out, 0);
    let flags = 2
        | 4
        | 8
        | 16
        | 64
        | 128
        | 512
        | if p.dashes.is_empty() { 0 } else { 32 | 256 }
        | if p.compound.is_empty() { 0 } else { 1024 };
    u32w(&mut out, flags);
    u32w(&mut out, 2);
    floats(&mut out, [p.width])?;
    for v in [p.start_cap, p.end_cap, p.join] {
        u32w(&mut out, v);
    }
    floats(&mut out, [p.miter_limit])?;
    if !p.dashes.is_empty() {
        u32w(&mut out, 5);
    }
    u32w(&mut out, p.dash_cap);
    floats(&mut out, [p.dash_offset])?;
    if !p.dashes.is_empty() {
        u32w(&mut out, p.dashes.len() as u32);
        floats(&mut out, p.dashes.iter().copied())?;
    }
    u32w(&mut out, p.alignment);
    if !p.compound.is_empty() {
        u32w(&mut out, p.compound.len() as u32);
        floats(&mut out, p.compound.iter().copied())?;
    }
    out.extend(brush_write(p, images)?);
    Ok(out)
}

fn pen_read(bytes: &[u8], images: &mut HashMap<u64, Bitmap>) -> Result<Paint, String> {
    let mut r = Reader::new(bytes);
    r.version()?;
    if r.u32()? != 0 {
        return Err("invalid EMF+ pen type".into());
    }
    let flags = r.u32()?;
    let unit = r.u32()?;
    let width = r.f32()?;
    if ![0, 2].contains(&unit) || flags & 0x1801 != 0 {
        return Err("unsupported EMF+ pen transform/unit/custom cap".into());
    }
    let start = if flags & 2 != 0 { r.u32()? } else { 0 };
    let end = if flags & 4 != 0 { r.u32()? } else { 0 };
    let join = if flags & 8 != 0 { r.u32()? } else { 0 };
    let miter = if flags & 16 != 0 { r.f32()? } else { 10. };
    let style = if flags & 32 != 0 { r.u32()? } else { 0 };
    let dash_cap = if flags & 64 != 0 { r.u32()? } else { 0 };
    let offset = if flags & 128 != 0 { r.f32()? } else { 0. };
    let mut dashes = match style {
        0 | 5 => vec![],
        1 => vec![3., 1.],
        2 => vec![1., 1.],
        3 => vec![3., 1., 1., 1.],
        4 => vec![3., 1., 1., 1., 1., 1.],
        _ => return Err("invalid pen line style".into()),
    };
    if flags & 256 != 0 {
        let n = r.count(4096)?;
        dashes.clear();
        for _ in 0..n {
            dashes.push(r.f32()?);
        }
    }
    let alignment = if flags & 512 != 0 { r.u32()? } else { 0 };
    let mut compound = vec![];
    if flags & 1024 != 0 {
        let n = r.count(4096)?;
        for _ in 0..n {
            compound.push(r.f32()?);
        }
    }
    let mut p = brush_read(&mut r, images)?;
    p.stroke = true;
    p.width = width;
    p.start_cap = start;
    p.end_cap = end;
    p.join = join;
    p.miter_limit = miter;
    p.dashes = dashes;
    p.dash_cap = dash_cap;
    p.dash_offset = offset;
    p.alignment = alignment;
    p.compound = compound;
    Ok(p)
}

fn bitmap_write(image: &Bitmap) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    for v in [
        VERSION,
        1,
        image.width,
        image.height,
        image.width * 4,
        0x26200a,
        0,
    ] {
        u32w(&mut out, v);
    }
    if image.rgba.len() != image.width as usize * image.height as usize * 4 {
        return Err("invalid bitmap storage".into());
    }
    for p in image.rgba.chunks_exact(4) {
        out.extend([p[2], p[1], p[0], p[3]]);
    }
    Ok(out)
}
fn bitmap_read(r: &mut Reader<'_>) -> Result<Bitmap, String> {
    r.version()?;
    if r.u32()? != 1 {
        return Err("nested EMF+ metafile image is unsupported".into());
    }
    let width = r.u32()?;
    let height = r.u32()?;
    let stride = r.u32()? as i32;
    let format = r.u32()?;
    let kind = r.u32()?;
    let length = (width as usize)
        .checked_mul(height as usize)
        .and_then(|v| v.checked_mul(4))
        .ok_or("bitmap length overflow")?;
    if width == 0
        || height == 0
        || length > MAX_BYTES
        || kind != 0
        || ![0x26200a, 0xe200b, 0x22009].contains(&format)
        || stride.unsigned_abs() as usize != width as usize * 4
    {
        return Err("unsupported EMF+ bitmap encoding".into());
    }
    let source = r.take(length)?;
    let mut rgba = vec![0; length];
    for y in 0..height as usize {
        let src_y = if stride < 0 {
            height as usize - 1 - y
        } else {
            y
        };
        for x in 0..width as usize {
            let src = &source[(src_y * width as usize + x) * 4..][..4];
            let dst = &mut rgba[(y * width as usize + x) * 4..][..4];
            dst.copy_from_slice(&[
                src[2],
                src[1],
                src[0],
                if format == 0x22009 { 255 } else { src[3] },
            ]);
            if format == 0xe200b && src[3] != 0 {
                for c in &mut dst[..3] {
                    *c = ((*c as u32 * 255 + src[3] as u32 / 2) / src[3] as u32).min(255) as u8;
                }
            }
        }
    }
    Ok(Bitmap {
        width,
        height,
        rgba,
    })
}

pub(super) fn encode(record: &Record) -> Result<Vec<u8>, String> {
    let mut data = Vec::new();
    let mut header = Vec::new();
    for v in [VERSION, 1, 96, 96] {
        u32w(&mut header, v);
    }
    plus(&mut data, 0x4001, 0, &header);
    plus(&mut data, 0x4030, 2, &1f32.to_le_bytes());
    plus(&mut data, 0x4009, 0, &record.background.to_le_bytes());
    for request in &record.operations {
        let mut matrix = Vec::new();
        floats(&mut matrix, request.transform)?;
        plus(&mut data, 0x402a, 0, &matrix);
        plus(
            &mut data,
            0x401e,
            if request.flags & 1 != 0 { 0 } else { 9 },
            &[],
        );
        for paint in &request.paints {
            let mut path = request.path.clone();
            for e in &mut path {
                let n = match e.verb {
                    0 | 1 => 1,
                    2 => 3,
                    _ => 0,
                };
                for p in e.points[..n * 2].chunks_exact_mut(2) {
                    p[0] += paint.offset[0];
                    p[1] += paint.offset[1];
                }
            }
            object(&mut data, 3, 0, &path_write(&path, request.fill_rule)?);
            if paint.stroke {
                object(&mut data, 2, 1, &pen_write(paint, &record.images)?);
                plus(&mut data, 0x4015, 0, &1u32.to_le_bytes());
            } else {
                object(&mut data, 1, 1, &brush_write(paint, &record.images)?);
                plus(&mut data, 0x4014, 0, &1u32.to_le_bytes());
            }
        }
        if data.len() > MAX_BYTES {
            return Err("metafile resource limit".into());
        }
    }
    plus(&mut data, 0x4002, 0, &[]);
    let mut out = vec![0u8; 108];
    let total = 108 + 16 + data.len() + 20;
    let [left, top, width, height] = record.bounds;
    for (offset, value) in [
        (0, 1),
        (4, 108),
        (8, left as i32 as u32),
        (12, top as i32 as u32),
        (16, (left + width) as i32 as u32),
        (20, (top + height) as i32 as u32),
        (24, (left * 2540. / 96.) as i32 as u32),
        (28, (top * 2540. / 96.) as i32 as u32),
        (32, ((left + width) * 2540. / 96.) as i32 as u32),
        (36, ((top + height) * 2540. / 96.) as i32 as u32),
        (40, 0x464d4520),
        (44, 0x10000),
        (48, total as u32),
        (52, 3),
        (56, 1),
        (72, record.width),
        (76, record.height),
        (80, (record.width as f64 * 25.4 / 96.).ceil() as u32),
        (84, (record.height as f64 * 25.4 / 96.).ceil() as u32),
    ] {
        out[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    u32w(&mut out, 70);
    u32w(&mut out, (data.len() + 16) as u32);
    u32w(&mut out, (data.len() + 4) as u32);
    u32w(&mut out, 0x2b464d45);
    out.extend(data);
    for v in [14, 20, 0, 0, 20] {
        u32w(&mut out, v);
    }
    Ok(out)
}

#[derive(Clone)]
enum Object {
    Brush(Paint),
    Pen(Paint),
    Path(Vec<PathElement>, u32),
}

pub(super) fn decode(bytes: &[u8]) -> Result<Record, String> {
    if bytes.len() < 88 || bytes.len() > MAX_BYTES {
        return Err("invalid EMF length".into());
    }
    let mut header = Reader::new(bytes);
    if header.u32()? != 1 {
        return Err("EMR_HEADER is missing".into());
    }
    let size = header.u32()? as usize;
    if size < 88 || size > bytes.len() || size % 4 != 0 {
        return Err("invalid EMR_HEADER size".into());
    }
    let bounds: [f64; 4] = [
        header.u32()? as i32 as f64,
        header.u32()? as i32 as f64,
        header.u32()? as i32 as f64,
        header.u32()? as i32 as f64,
    ];
    header.take(16)?;
    if header.u32()? != 0x464d4520 {
        return Err("invalid EMF signature".into());
    }
    header.take(4)?;
    if header.u32()? as usize != bytes.len() {
        return Err("EMF byte count mismatch".into());
    }
    let mut record = Record::new(
        (bounds[2] - bounds[0]).ceil().max(1.) as u32,
        (bounds[3] - bounds[1]).ceil().max(1.) as u32,
        0,
    )?;
    record.bounds = [
        bounds[0],
        bounds[1],
        bounds[2] - bounds[0],
        bounds[3] - bounds[1],
    ];
    let mut parser = Parser::new(&mut record);
    let mut at = size;
    let mut eof = false;
    while at < bytes.len() {
        let mut r = Reader::new(&bytes[at..]);
        let kind = r.u32()?;
        let size = r.u32()? as usize;
        if size < 8 || size % 4 != 0 || size > bytes.len() - at {
            return Err("invalid EMF record length".into());
        }
        if kind == 70 {
            let count = r.count(MAX_BYTES)?;
            if count > size - 12 {
                return Err("invalid EMF comment length".into());
            }
            let comment = r.take(count)?;
            if comment.starts_with(b"EMF+") {
                parser.parse(&comment[4..])?;
            }
        } else if kind == 14 {
            if size < 20 || at + size != bytes.len() {
                return Err("invalid EMR_EOF".into());
            }
            eof = true;
        }
        at += size;
    }
    if !eof || !parser.header || !parser.eof || !parser.pending.is_empty() {
        return Err("incomplete EMF+ metafile".into());
    }
    Ok(record)
}

struct Parser<'a> {
    record: &'a mut Record,
    objects: HashMap<u8, Object>,
    pending: HashMap<u8, (u8, usize, Vec<u8>)>,
    transform: [f64; 6],
    flags: u32,
    clip: [i32; 4],
    saved: Vec<(u32, [f64; 6], u32, [i32; 4])>,
    header: bool,
    eof: bool,
    retained_points: usize,
}
impl<'a> Parser<'a> {
    fn new(record: &'a mut Record) -> Self {
        Self {
            record,
            objects: HashMap::new(),
            pending: HashMap::new(),
            transform: IDENTITY,
            flags: 0,
            clip: [i32::MIN / 4, i32::MIN / 4, i32::MAX / 4, i32::MAX / 4],
            saved: vec![],
            header: false,
            eof: false,
            retained_points: 0,
        }
    }
    fn emit(&mut self, path: Vec<PathElement>, fill_rule: u32, paint: Paint) -> Result<(), String> {
        self.retained_points = self
            .retained_points
            .checked_add(path.len())
            .ok_or("metafile point limit")?;
        if self.record.operations.len() >= 100_000 || self.retained_points > MAX_POINTS {
            return Err("metafile operation limit".into());
        }
        self.record.operations.push(DrawRequest {
            target: 0,
            clip: self.clip,
            transform: self.transform,
            path,
            fill_rule,
            paints: vec![paint],
            alpha_mode: 0,
            flags: self.flags,
        });
        Ok(())
    }
    fn brush(&self, flags: u16, id: u32) -> Result<Paint, String> {
        if flags & 0x8000 != 0 {
            return Ok(solid(id));
        }
        match self.objects.get(&(id as u8)).filter(|_| id < 64) {
            Some(Object::Brush(p)) => Ok(p.clone()),
            _ => Err("missing EMF+ brush object".into()),
        }
    }
    fn object(&mut self, flags: u16, data: &[u8]) -> Result<(), String> {
        let id = flags as u8;
        let kind = ((flags >> 8) & 0x7f) as u8;
        if id >= 64 {
            return Err("EMF+ object ID out of range".into());
        }
        let mut r = Reader::new(data);
        let continued = flags & 0x8000 != 0;
        let total = if continued {
            Some(r.count(MAX_BYTES)?)
        } else {
            None
        };
        let chunk = &data[r.at..];
        let object_data =
            if let Some((previous_kind, expected, mut pending)) = self.pending.remove(&id) {
                if previous_kind != kind
                    || total.is_some_and(|n| n != expected)
                    || chunk.len() > expected - pending.len()
                {
                    return Err("invalid continued EMF+ object".into());
                }
                pending.extend(chunk);
                if continued {
                    if pending.len() >= expected {
                        return Err("continued object exceeds declared size".into());
                    }
                    self.pending.insert(id, (kind, expected, pending));
                    return Ok(());
                }
                if pending.len() != expected {
                    return Err("incomplete continued EMF+ object".into());
                }
                pending
            } else if let Some(total) = total {
                if total <= chunk.len() {
                    return Err("invalid continued object size".into());
                }
                self.pending.insert(id, (kind, total, chunk.to_vec()));
                return Ok(());
            } else {
                chunk.to_vec()
            };
        let mut r = Reader::new(&object_data);
        let object = match kind {
            1 => Object::Brush(brush_read(&mut r, &mut self.record.images)?),
            2 => Object::Pen(pen_read(&object_data, &mut self.record.images)?),
            3 => {
                let (p, f) = path_read(&object_data)?;
                Object::Path(p, f)
            }
            _ => return Err(format!("unsupported EMF+ object type {kind}")),
        };
        self.objects.insert(id, object);
        Ok(())
    }
    fn parse(&mut self, bytes: &[u8]) -> Result<(), String> {
        let mut at = 0;
        while at < bytes.len() {
            let mut h = Reader::new(&bytes[at..]);
            let kind = h.u16()?;
            let flags = h.u16()?;
            let size = h.u32()? as usize;
            let count = h.u32()? as usize;
            if self.eof
                || size < 12
                || size % 4 != 0
                || size > bytes.len() - at
                || count > size - 12
            {
                return Err("invalid EMF+ record length".into());
            }
            if !self.header && kind != 0x4001 {
                return Err("EMF+ header is missing".into());
            }
            let data = &bytes[at + 12..at + 12 + count];
            let mut r = Reader::new(data);
            match kind {
                0x4001 => {
                    if self.header {
                        return Err("duplicate EMF+ header".into());
                    }
                    r.version()?;
                    r.take(12)?;
                    self.header = true;
                }
                0x4002 => self.eof = true,
                0x4003 => {}
                0x4004 => return Err("EMF+ GetDC drawing is unsupported".into()),
                0x4008 => self.object(flags, data)?,
                0x4009 => {
                    self.record.background = r.u32()?;
                    self.record.operations.clear();
                }
                0x4014 | 0x4015 => {
                    let id = r.u32()?;
                    let paint = if kind == 0x4014 {
                        self.brush(flags, id)?
                    } else {
                        match self.objects.get(&(id as u8)).filter(|_| id < 64) {
                            Some(Object::Pen(p)) => p.clone(),
                            _ => return Err("missing EMF+ pen object".into()),
                        }
                    };
                    let (path, fill) = match self.objects.get(&(flags as u8)) {
                        Some(Object::Path(p, f)) => (p.clone(), *f),
                        _ => return Err("missing EMF+ path object".into()),
                    };
                    self.emit(path, fill, paint)?;
                }
                0x401e => self.flags = u32::from(flags & 1 == 0),
                0x4023 if flags & 255 != 0 => {
                    return Err("EMF+ source-copy compositing is unsupported".into());
                }
                0x401f..=0x4024 => {}
                0x4025 => {
                    let id = r.u32()?;
                    if self.saved.len() >= 1024 {
                        return Err("metafile state depth limit".into());
                    }
                    self.saved.push((id, self.transform, self.flags, self.clip));
                }
                0x4026 => {
                    let id = r.u32()?;
                    let n = self
                        .saved
                        .iter()
                        .rposition(|s| s.0 == id)
                        .ok_or("missing EMF+ saved state")?;
                    let (_, m, f, c) = self.saved[n];
                    self.transform = m;
                    self.flags = f;
                    self.clip = c;
                    self.saved.truncate(n);
                }
                0x402a => self.transform = r.floats()?,
                0x402b => self.transform = IDENTITY,
                0x402c..=0x402f => {
                    let matrix = match kind {
                        0x402c => r.floats()?,
                        0x402d => {
                            let [x, y] = r.floats()?;
                            [1., 0., 0., 1., x, y]
                        }
                        0x402e => {
                            let [x, y] = r.floats()?;
                            [x, 0., 0., y, 0., 0.]
                        }
                        _ => {
                            let angle = r.f32()?.to_radians();
                            let (s, c) = angle.sin_cos();
                            [c, s, -s, c, 0., 0.]
                        }
                    };
                    self.transform = if flags & 0x2000 != 0 {
                        multiply(self.transform, matrix)
                    } else {
                        multiply(matrix, self.transform)
                    };
                }
                0x4030 => {
                    let scale = r.f32()?;
                    if flags & 255 != 2 || scale != 1. {
                        return Err("nonpixel EMF+ page transform is unsupported".into());
                    }
                }
                0x4031 => self.clip = [i32::MIN / 4, i32::MIN / 4, i32::MAX / 4, i32::MAX / 4],
                _ => return Err(format!("unsupported EMF+ record 0x{kind:04x}")),
            }
            at += size;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn triangle() -> Vec<PathElement> {
        [
            (0, [1., 2., 0., 0., 0., 0.]),
            (1, [10., 2., 0., 0., 0., 0.]),
            (2, [12., 3., 5., 8., 1., 2.]),
            (3, [0.; 6]),
        ]
        .into_iter()
        .map(|(verb, points)| PathElement {
            verb,
            points,
            reserved: 0,
        })
        .collect()
    }
    #[test]
    fn microsoft_specification_path_example() {
        let bytes = include_bytes!("../../tests/fixtures/layer_ex_draw/ms-emfplus-path.bin");
        let mut record = Record::new(640, 480, 0).unwrap();
        let mut parser = Parser::new(&mut record);
        parser.header = true;
        parser.parse(bytes).unwrap();
        parser
            .parse(&[
                0x14, 0x40, 0, 0x80, 16, 0, 0, 0, 4, 0, 0, 0, 255, 255, 255, 0,
            ])
            .unwrap();
        assert_eq!(parser.record.operations.len(), 1);
        assert_eq!(
            parser.record.operations[0]
                .path
                .iter()
                .filter(|p| p.verb == 2)
                .count(),
            5
        );
        assert_eq!(parser.record.operations[0].paints[0].colors[0], 0x00ffffff);
    }
    #[test]
    fn independent_wine_gdiplus_path_metafile() {
        let record = decode(include_bytes!(
            "../../tests/fixtures/layer_ex_draw/wine-path.emf"
        ))
        .unwrap();
        assert_eq!(record.background, 0xff112233);
        assert_eq!(record.operations.len(), 2);
        assert_eq!(record.operations[0].paints[0].colors[0], 0xc8e65028);
        assert_eq!(record.operations[1].paints[0].dashes, [3., 1.]);
        assert_eq!(record.operations[1].paints[0].width, 2.5);
    }
    #[test]
    fn emfplus_vector_roundtrip() {
        let mut record = Record::new(32, 24, 0xff102030).unwrap();
        let mut paint = solid(0xffaa2233);
        paint.stroke = true;
        paint.width = 2.5;
        paint.dashes = vec![2., 3.];
        paint.compound = vec![0., 0.2, 0.8, 1.];
        record.operations.push(DrawRequest {
            target: 0,
            clip: [0, 0, 32, 24],
            transform: [2., 0., 0., 3., 4., 5.],
            path: triangle(),
            fill_rule: 1,
            paints: vec![solid(0x88776655), paint],
            alpha_mode: 0,
            flags: 0,
        });
        let bytes = encode(&record).unwrap();
        let decoded = decode(&bytes).unwrap();
        assert_eq!(decoded.operations.len(), 2);
        assert_eq!(decoded.background, record.background);
        assert_eq!(decoded.operations[1].path.len(), 4);
        assert_eq!(decoded.operations[1].fill_rule, 1);
        assert_eq!(decoded.operations[1].paints[0].dashes, [2., 3.]);
        assert_eq!(
            decoded.operations[0].transform,
            record.operations[0].transform
        );
    }
    #[test]
    fn rejects_lengths_and_truncation() {
        let record = Record::new(10, 10, 0).unwrap();
        let bytes = encode(&record).unwrap();
        for n in 0..bytes.len() {
            assert!(decode(&bytes[..n]).is_err(), "accepted truncation {n}");
        }
        for offset in [4, 48, 112, 116, 128] {
            let mut bad = bytes.clone();
            bad[offset..offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
            assert!(decode(&bad).is_err(), "accepted bad length at {offset}");
        }
    }
    #[test]
    fn continued_objects_validate_total_and_identity() {
        let mut record = Record::new(10, 10, 0).unwrap();
        let mut parser = Parser::new(&mut record);
        let data = path_write(&triangle(), 0).unwrap();
        let cut = 16;
        let mut first = Vec::new();
        u32w(&mut first, data.len() as u32);
        first.extend(&data[..cut]);
        parser.object(0x8300, &first).unwrap();
        parser.object(0x300, &data[cut..]).unwrap();
        assert!(matches!(parser.objects.get(&0), Some(Object::Path(_, 0))));
        parser.object(0x8300, &first).unwrap();
        assert!(parser.object(0x100, &data[cut..]).is_err());
        parser.object(0x8300, &first).unwrap();
        assert!(parser.object(0x300, &data[cut..data.len() - 4]).is_err());
    }
}
