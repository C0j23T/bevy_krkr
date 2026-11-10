use crate::ffi::KrkrPngInfo;
use std::{
    collections::{BTreeSet, HashMap},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicI32, Ordering},
    },
    thread::JoinHandle,
};

pub struct Options {
    pub format: u32,
    pub level: i32,
    pub meta: KrkrPngInfo,
    pub tags: Vec<u8>,
}
pub struct Snapshot {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

pub fn encode(
    image: &Snapshot,
    options: &Options,
    canceled: &AtomicBool,
    progress: &AtomicI32,
) -> Result<Vec<u8>, String> {
    if canceled.load(Ordering::Acquire) {
        return Ok(Vec::new());
    }
    let result = if options.format == 1 {
        tlg(image, &options.tags, canceled, progress)
    } else {
        png(image, options)
    }?;
    progress.store(100, Ordering::Release);
    Ok(result)
}
fn png(image: &Snapshot, options: &Options) -> Result<Vec<u8>, String> {
    use png::{
        BitDepth, ColorType, DeflateCompression, Encoder, PixelDimensions, Unit, chunk::ChunkType,
    };
    if !(-1..=9).contains(&options.level) {
        return Err("invalid PNG compression level".into());
    }
    let mut bytes = Vec::new();
    let mut encoder = Encoder::new(&mut bytes, image.width, image.height);
    encoder.set_depth(BitDepth::Eight);
    encoder.set_deflate_compression(match options.level {
        0 => DeflateCompression::NoCompression,
        -1 => DeflateCompression::Level(6),
        n => DeflateCompression::Level(n as u8),
    });
    if options.format == 2 {
        encoder.set_color(ColorType::Indexed);
        let mut palette = Vec::with_capacity(768);
        for i in 0..256 {
            palette.extend_from_slice(&[
                [0, 64, 96, 128, 160, 192, 224, 255][(i >> 2) & 7],
                [0, 64, 96, 128, 160, 192, 224, 255][(i >> 5) & 7],
                [0, 128, 192, 255][i & 3],
            ]);
        }
        encoder.set_palette(palette);
        let mut alpha = vec![255; 256];
        alpha[0] = 0;
        encoder.set_trns(alpha);
    } else {
        encoder.set_color(ColorType::Rgba);
    }
    let meta = &options.meta;
    if meta.has_resolution != 0 {
        encoder.set_pixel_dims(Some(PixelDimensions {
            xppu: meta.resolution_x,
            yppu: meta.resolution_y,
            unit: if meta.resolution_unit == 1 {
                Unit::Meter
            } else {
                Unit::Unspecified
            },
        }));
    }
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    if meta.has_offset != 0 {
        let mut chunk = Vec::new();
        chunk.extend_from_slice(&meta.offset_x.to_be_bytes());
        chunk.extend_from_slice(&meta.offset_y.to_be_bytes());
        chunk.push(meta.offset_unit as u8);
        writer
            .write_chunk(ChunkType(*b"oFFs"), &chunk)
            .map_err(|e| e.to_string())?;
    }
    if meta.has_vpage != 0 {
        let mut chunk = Vec::new();
        chunk.extend_from_slice(&meta.vpage_width.to_be_bytes());
        chunk.extend_from_slice(&meta.vpage_height.to_be_bytes());
        chunk.push(meta.vpage_unit as u8);
        writer
            .write_chunk(ChunkType(*b"vpAg"), &chunk)
            .map_err(|e| e.to_string())?;
    }
    writer
        .write_image_data(&image.pixels)
        .map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())?;
    Ok(bytes)
}

#[derive(Clone)]
struct Slide {
    dictionary: [u8; 4096],
    position: usize,
    pairs: HashMap<u16, BTreeSet<usize>>,
}
impl Slide {
    fn new() -> Self {
        Self {
            dictionary: [0; 4096],
            position: 0,
            pairs: HashMap::from([(0, (0..4096).collect())]),
        }
    }
    fn key(&self, p: usize) -> u16 {
        u16::from_le_bytes([self.dictionary[p], self.dictionary[(p + 1) & 4095]])
    }
    fn insert(&mut self, byte: u8) {
        let p = self.position;
        let prev = (p + 4095) & 4095;
        for at in [prev, p] {
            let key = self.key(at);
            if let Some(set) = self.pairs.get_mut(&key) {
                set.remove(&at);
            }
        }
        self.dictionary[p] = byte;
        for at in [prev, p] {
            let key = self.key(at);
            self.pairs.entry(key).or_default().insert(at);
        }
        self.position = (p + 1) & 4095;
    }
    fn compress(&mut self, input: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < input.len() {
            let flag = out.len();
            out.push(0);
            for bit in 0..8 {
                if i == input.len() {
                    break;
                }
                let mut best = (0, 0);
                if input.len() - i >= 3 {
                    if let Some(candidates) = self
                        .pairs
                        .get(&u16::from_le_bytes([input[i], input[i + 1]]))
                    {
                        for &p in candidates.iter().rev().take(64) {
                            let limit = (input.len() - i)
                                .min(273)
                                .min((self.position + 4096 - p) & 4095);
                            let mut n = 0;
                            while n < limit && self.dictionary[(p + n) & 4095] == input[i + n] {
                                n += 1;
                            }
                            if n > best.1 {
                                best = (p, n);
                            }
                        }
                    }
                }
                let count = if best.1 >= 3 {
                    out[flag] |= 1 << bit;
                    out.push(best.0 as u8);
                    out.push(((best.0 >> 8) as u8) | (((best.1 - 3).min(15) as u8) << 4));
                    if best.1 >= 18 {
                        out.push((best.1 - 18) as u8);
                    }
                    best.1
                } else {
                    out.push(input[i]);
                    1
                };
                for &byte in &input[i..i + count] {
                    self.insert(byte);
                }
                i += count;
            }
        }
        out
    }
}
fn tlg(
    image: &Snapshot,
    tags: &[u8],
    canceled: &AtomicBool,
    progress: &AtomicI32,
) -> Result<Vec<u8>, String> {
    let w = image.width as usize;
    let h = image.height as usize;
    let blocks = h.div_ceil(4);
    let mut raw = b"TLG5.0\0raw\x1a".to_vec();
    raw.push(4);
    for n in [image.width, image.height, 4] {
        raw.extend_from_slice(&n.to_le_bytes());
    }
    let table = raw.len();
    raw.resize(table + blocks * 4, 0);
    let mut slide = Slide::new();
    for (block, y0) in (0..h).step_by(4).enumerate() {
        if canceled.load(Ordering::Acquire) {
            return Ok(Vec::new());
        }
        progress.store((y0 * 100 / h) as i32, Ordering::Release);
        let mut channels: [Vec<u8>; 4] = std::array::from_fn(|_| Vec::with_capacity(w * 4));
        for y in y0..(y0 + 4).min(h) {
            let mut prev = [0u8; 4];
            for x in 0..w {
                let mut value = [0u8; 4];
                for (c, index) in [2, 1, 0, 3].into_iter().enumerate() {
                    let at = (y * w + x) * 4 + index;
                    let vertical = image.pixels[at].wrapping_sub(if y == 0 {
                        0
                    } else {
                        image.pixels[at - w * 4]
                    });
                    value[c] = vertical.wrapping_sub(prev[c]);
                    prev[c] = vertical;
                }
                for (c, byte) in [
                    value[0].wrapping_sub(value[1]),
                    value[1],
                    value[2].wrapping_sub(value[1]),
                    value[3],
                ]
                .into_iter()
                .enumerate()
                {
                    channels[c].push(byte);
                }
            }
        }
        let start = raw.len();
        for channel in channels {
            let mut trial = slide.clone();
            let compressed = trial.compress(&channel);
            let bytes = if compressed.len() < channel.len() {
                slide = trial;
                raw.push(0);
                compressed
            } else {
                raw.push(1);
                channel
            };
            raw.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
            raw.extend_from_slice(&bytes);
        }
        let size = (raw.len() - start) as u32;
        raw[table + block * 4..table + block * 4 + 4].copy_from_slice(&size.to_le_bytes());
    }
    if tags.is_empty() {
        return Ok(raw);
    }
    let mut out = b"TLG0.0\0sds\x1a".to_vec();
    out.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    out.extend(raw);
    out.extend_from_slice(b"tags");
    out.extend_from_slice(&(tags.len() as u32).to_le_bytes());
    out.extend_from_slice(tags);
    Ok(out)
}

pub struct Job {
    pub canceled: Arc<AtomicBool>,
    pub progress: Arc<AtomicI32>,
    thread: Option<JoinHandle<Result<Vec<u8>, String>>>,
}
impl Job {
    pub fn start(image: Snapshot, options: Options) -> Result<Self, String> {
        let canceled = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(AtomicI32::new(0));
        let c = canceled.clone();
        let p = progress.clone();
        let thread = std::thread::Builder::new()
            .name("krkr-save-image".into())
            .spawn(move || encode(&image, &options, &c, &p))
            .map_err(|e| e.to_string())?;
        Ok(Self {
            canceled,
            progress,
            thread: Some(thread),
        })
    }
    pub fn finished(&self) -> bool {
        self.thread.as_ref().is_some_and(|t| t.is_finished())
    }
    pub fn finish(&mut self) -> Result<Vec<u8>, String> {
        self.thread
            .take()
            .ok_or("save task already consumed")?
            .join()
            .map_err(|_| "image encoder panicked")?
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        self.canceled.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
pub static JOBS: Mutex<(u64, Option<HashMap<u64, Job>>)> = Mutex::new((1, None));
