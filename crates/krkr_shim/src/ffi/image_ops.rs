use super::{guard, input, output};
use std::ffi::c_void;

/// Runs explicit pixel analysis or editing on BGRA snapshots.
/// # Safety
/// Buffers must match their lengths; params has 6 integers and values has 7.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_image_operation(
    op: u32,
    p: *mut c_void,
    len: usize,
    w: u32,
    h: u32,
    base: *const c_void,
    base_len: usize,
    bw: u32,
    bh: u32,
    params: *const i64,
    values: *mut i64,
    bytes: *mut *mut c_void,
    bytes_len: *mut usize,
) -> i32 {
    guard(-1, || {
        if p.is_null()
            || params.is_null()
            || values.is_null()
            || bytes.is_null()
            || bytes_len.is_null()
            || len != (w as usize) * (h as usize) * 4
            || w == 0
            || h == 0
        {
            return -1;
        }
        let args = unsafe { std::slice::from_raw_parts(params, 6) };
        let values = unsafe { std::slice::from_raw_parts_mut(values, 7) };
        values.fill(0);
        let pixels = unsafe { output(p, len) };
        let source = unsafe { input(base, base_len) };
        let (w, h) = (w as usize, h as usize);
        if matches!(op, 2 | 3) && (bw as usize != w || bh as usize != h || source.len() != len) {
            return -1;
        }
        match op {
            0..=3 => {
                let mut rect = [w, h, 0, 0];
                let mut count = 0;
                for (i, pixel) in pixels.chunks_exact_mut(4).enumerate() {
                    let active = match op {
                        0 => pixel[3] != 0,
                        1 => pixel.iter().any(|&v| v != 0),
                        _ => {
                            pixel[3] != source[i * 4 + 3]
                                || pixel[3] != 0 && pixel != &source[i * 4..i * 4 + 4]
                        }
                    };
                    if active {
                        count += 1;
                        rect[0] = rect[0].min(i % w);
                        rect[1] = rect[1].min(i / w);
                        rect[2] = rect[2].max(i % w + 1);
                        rect[3] = rect[3].max(i / w + 1);
                    }
                    if op == 3 {
                        let fill = if active { args[1] } else { args[0] };
                        if fill >= 0 {
                            pixel.copy_from_slice(&(fill as u32).to_le_bytes());
                        }
                    }
                }
                if count != 0 {
                    values[..4].copy_from_slice(&[
                        rect[0] as i64,
                        rect[1] as i64,
                        (rect[2] - rect[0]) as i64,
                        (rect[3] - rect[1]) as i64,
                    ]);
                }
                values[4] = count;
            }
            4 => {
                let level = args[0];
                if level <= 0 {
                    return -1;
                }
                let threshold = (args[1] as u8).max(1);
                let fill = (args[2] as u32).to_le_bytes();
                let mut known: Vec<bool> =
                    pixels.chunks_exact(4).map(|p| p[3] >= threshold).collect();
                for (i, p) in pixels.chunks_exact_mut(4).enumerate() {
                    if !known[i] {
                        p[..3].copy_from_slice(&fill[..3]);
                    }
                }
                for _ in 0..level {
                    let mut added = Vec::new();
                    for i in 0..w * h {
                        if known[i] {
                            continue;
                        }
                        let x = i % w;
                        let y = i / w;
                        let mut sum = [0u32; 3];
                        let mut n = 0;
                        for neighbor in [
                            if x > 0 { Some(i - 1) } else { None },
                            if x + 1 < w { Some(i + 1) } else { None },
                            if y > 0 { Some(i - w) } else { None },
                            if y + 1 < h { Some(i + w) } else { None },
                        ]
                        .into_iter()
                        .flatten()
                        {
                            if known[neighbor] {
                                for c in 0..3 {
                                    sum[c] += pixels[neighbor * 4 + c] as u32;
                                }
                                n += 1;
                            }
                        }
                        if n > 0 {
                            for c in 0..3 {
                                pixels[i * 4 + c] = (sum[c] / n) as u8;
                            }
                            added.push(i);
                        }
                    }
                    if added.is_empty() {
                        break;
                    }
                    for i in added {
                        known[i] = true;
                    }
                }
            }
            5 => {
                if source.len() != bw as usize * bh as usize * 4 {
                    return -1;
                }
                for y in 0..h.min(bh as usize) {
                    for x in 0..w.min(bw as usize) {
                        pixels[(y * w + x) * 4 + 3] = source[(y * bw as usize + x) * 4];
                    }
                }
            }
            6 | 8 => {
                let l = args[0].max(0).min(w as i64) as usize;
                let t = args[1].max(0).min(h as i64) as usize;
                let r = (args[0].saturating_add(args[2])).max(0).min(w as i64) as usize;
                let b = (args[1].saturating_add(args[3])).max(0).min(h as i64) as usize;
                let mut sum = [0u64; 4];
                let mut count = 0;
                let mut blank = true;
                for y in t..b {
                    for x in l..r {
                        let p = &pixels[(y * w + x) * 4..][..4];
                        blank &= p[0] == 0;
                        for c in 0..4 {
                            sum[c] += p[c] as u64;
                        }
                        count += 1;
                    }
                }
                if op == 6 {
                    values[0] = blank as i64;
                } else {
                    if count == 0 {
                        return -1;
                    }
                    let mut color = 0u32;
                    for (c, v) in sum.into_iter().enumerate() {
                        color |= ((v / count) as u32) << (24 - c * 8);
                    }
                    values[0] = color as i32 as i64;
                }
            }
            7 => {
                let fill = (args[1] as u32 & 0xffffff).to_le_bytes();
                for p in pixels.chunks_exact_mut(4) {
                    if p[3] as i64 <= args[0] {
                        p.copy_from_slice(&fill);
                    }
                }
            }
            9 => {
                let mut hash = 0xcbf29ce484222325u64;
                for p in pixels.chunks_exact(4) {
                    for &v in p {
                        hash = (hash
                            ^ if args[0] != 0 && p[3] == 0 {
                                0
                            } else {
                                v as u64
                            })
                        .wrapping_mul(0x100000001b3);
                    }
                }
                for shift in [24, 16, 8, 0] {
                    for v in [w, h] {
                        hash = (hash ^ ((v >> shift) & 255) as u64).wrapping_mul(0x100000001b3);
                    }
                }
                values[0] = hash as i64;
            }
            10 => {
                if args[0] <= 0 || args[1] <= 0 {
                    return -1;
                }
                let sw = w.div_ceil(args[0] as usize);
                let sh = h.div_ceil(args[1] as usize);
                let mut out = Vec::new();
                for y in (0..h).step_by(sh) {
                    for x in (0..w).step_by(sw) {
                        let mut sum = [0u64; 4];
                        let n = (sh.min(h - y) * sw.min(w - x)) as u64;
                        for yy in y..(y + sh).min(h) {
                            for xx in x..(x + sw).min(w) {
                                let p = &pixels[(yy * w + xx) * 4..][..4];
                                if p[3] != 0 {
                                    for c in 0..4 {
                                        sum[c] += p[c] as u64;
                                    }
                                }
                            }
                        }
                        for v in sum {
                            out.push(((v + n / 2) / n) as u8);
                        }
                    }
                }
                unsafe {
                    *bytes_len = out.len();
                    *bytes = Box::into_raw(out.into_boxed_slice()) as *mut u8 as *mut c_void;
                }
            }
            11 => {
                for pixel in pixels.chunks_exact_mut(4) {
                    pixel.swap(0, 2);
                }
            }
            _ => return -1,
        }
        0
    })
}
/// Computes the seven upstream octet vector sums.
/// # Safety
/// Inputs must be readable and values must hold 7 writable integers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_octet_vector_sum(
    a: *const c_void,
    len: usize,
    b: *const c_void,
    b_len: usize,
    values: *mut i64,
) -> i32 {
    guard(-1, || {
        if values.is_null() || !b.is_null() && len != b_len {
            return -1;
        }
        let a = unsafe { input(a, len) };
        let b = unsafe { input(b, b_len) };
        let v = unsafe { std::slice::from_raw_parts_mut(values, 7) };
        v.fill(0);
        for (i, &x) in a.iter().enumerate() {
            let x = x as i64;
            let y = b.get(i).copied().unwrap_or(0) as i64;
            v[0] += x * y;
            v[1] += (x - y) * (x - y);
            v[2] += x * x;
            v[3] += y * y;
            v[4] += x;
            v[5] += y;
            v[6] += (x != y) as i64;
        }
        0
    })
}
