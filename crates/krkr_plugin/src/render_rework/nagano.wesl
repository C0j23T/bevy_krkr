import krkr::operations::{destination, nagano_tables, p, records, rule, source};

const nagano_motion: u32 = 256u;

fn fetch(t: texture_2d<f32>, at: vec2<i32>) -> vec4<f32> {
    return textureLoad(t, clamp(at, vec2(0), vec2<i32>(textureDimensions(t)) - 1), 0);
}

fn ns(q: vec2<i32>, which: i32) -> vec4<f32> {
    if (which == 0) {
        return fetch(source, q + vec2<i32>(p[6].xy));
    }
    return fetch(destination, q + vec2<i32>(p[7].xy));
}

fn blend_channel(a: u32, b: u32, opa: u32, shift: u32) -> u32 {
    if (shift == 24u) {
        return (a + (((b - a) * opa) >> 8u)) & 255u;
    }
    let x = a << shift;
    let y = b << shift;
    return ((x + (((y - x) * opa) >> 8u)) >> shift) & 255u;
}

fn nb(a: vec4<f32>, b: vec4<f32>, f: f32) -> vec4<f32> {
    let aa = vec4<u32>(round(a * 255.0));
    let bb = vec4<u32>(round(b * 255.0));
    let opa = u32(max(f, 0.0));
    return vec4<f32>(
        f32(blend_channel(aa.r, bb.r, opa, 16u)),
        f32(blend_channel(aa.g, bb.g, opa, 8u)),
        f32(blend_channel(aa.b, bb.b, opa, 0u)),
        f32(blend_channel(aa.a, bb.a, opa, 24u)),
    ) / 255.0;
}

fn dark(c: vec4<f32>, f: i32) -> vec4<f32> {
    let b = round(c * 255.0);
    return vec4(floor(b.rg * f32(256 - f) / 256.0), b.b - floor(b.b * f32(f) / 256.0), b.a) / 255.0;
}

fn light(c: vec4<f32>, f: i32) -> vec4<f32> {
    let b = round(c * 255.0);
    return vec4((b.rgb + floor((255.0 - b.rgb) * f32(f) / 256.0)) / 255.0, c.a);
}

public fn nagano(q: vec2<i32>) -> vec4<f32> {
    let w = i32(p[3].x);
    let h = i32(p[3].y);
    let v = p[3].zw;
    let z = p[4];
    let tail = p[5].xy;
    let a = ns(q, 0);
    let b = ns(q, 1);
    let k = i32(p[2].x);

    if (k == 0) {
        let pos = i32(v.x);
        if ((q.y & 1) == 0) {
            if (q.x < pos) {
                return ns(q + vec2(w - pos, 0), 1);
            }
            return ns(q - vec2(pos, 0), 0);
        }
        if (q.x < w - pos) {
            return ns(q + vec2(pos, 0), 0);
        }
        return ns(q - vec2(w - pos, 0), 1);
    }

    if (k == 1) {
        let center = vec2<f32>(f32(w), f32(h)) * 0.5;
        let last = vec2<i32>(w, h);
        let qa = vec2<i32>(center + (vec2<f32>(q) - center) * v.y);
        let qb = vec2<i32>(center + (vec2<f32>(q) - center) * z.x);
        var aa = vec4(0.0);
        var bb = vec4(0.0);
        if (all(qa >= vec2<i32>(0)) && all(qa < last)) {
            aa = ns(qa, 0);
        }
        if (all(qb >= vec2<i32>(0)) && all(qb < last)) {
            bb = ns(qb, 1);
        }
        if (bb.a == 0.0) {
            return vec4(aa.rgb, floor(round(aa.a * 255.0) * (255.0 - v.x) / 256.0) / 255.0);
        }
        if (aa.a == 0.0) {
            return vec4(bb.rgb, floor(round(bb.a * 255.0) * v.x / 256.0) / 255.0);
        }
        return nb(aa, bb, v.x);
    }

    if (k == 2) {
        let f = vec4(v, z.xy);
        var c = floor(round(a * 255.0) + (round(b * 255.0) - round(a * 255.0)) * f / 256.0);
        if (a.a == 0.0 || b.a == 0.0) {
            c = vec4<f32>(vec3<f32>(vec3<i32>(round(a.rgb * 255.0) * f.rgb) & vec3(255)), floor(round(b.a * 255.0) * f.a / 256.0));
            if (b.a == 0.0) {
                c.a = round(a.a * 255.0) - floor(round(a.a * 255.0) * f.a / 256.0);
            }
        }
        return c / 255.0;
    }

    if (k == 3) {
        let r = p[26u + u32(q.x)];
        let sy = ((i32(r.w) * (q.y - h / 2)) >> 8) + h / 2;
        if (r.x < 0.0 || sy < 0 || sy >= h) {
            return vec4(0.0);
        }
        let selected = i32(r.x);
        if (selected == 0) {
            return ns(vec2<i32>(i32(r.y), sy), 0);
        }
        if (selected == 1) {
            return ns(vec2<i32>(i32(r.z), sy), 1);
        }
        return vec4(0.0);
    }

    if (k == 4) {
        let phase = i32(v.x);
        let cs = i32(v.y);
        if (phase == 0) {
            return a;
        }
        if (phase == w + h) {
            return b;
        }
        let rb = min(w, h - q.y - phase + w);
        let base = h - phase - cs;
        let mid = base + 8;
        if (q.y < cs && q.x < max(w - phase + 8, 0)) {
            return b;
        }
        var c = b;
        if (q.y < cs) {
            c = b;
        } else if (q.x >= cs && q.x < rb) {
            c = ns(q - vec2(cs), 0);
        }
        if (q.y > max(base, 0) && q.y <= mid) {
            if (q.x >= max(w - phase + 12, 0) && q.x < rb) {
                c = nb(c, vec4(0.0, 0.0, 0.0, 1.0), f32(((q.y - max(base, 0)) * 63 / 8) & 255));
            }
        } else if (q.y > mid && q.y <= mid + w) {
            let cb = w - phase + 8;
            let start = max(cb, 0);
            var lc = h - cs - 1;
            if (cb < 0) {
                lc += cb;
            }
            let end = min(rb, lc + start);
            if (q.x >= start && q.x < end) {
                let index = clamp(q.y - mid, 0, w - 1) * h - cb + q.x;
                let bp = fetch(source, vec2(index / h, h - 1 - index % h));
                let back = nb(bp, vec4(z.zw, tail.x, 255.0) / 255.0, z.y);
                c = nb(c, back, z.x);
            }
            if (q.x >= max(start, end - 24) && q.x < end - 16) {
                c = nb(c, vec4(1.0), f32((q.x - (end - 16)) * 127 / 8 + 127));
            }
            if (q.x >= max(start, end - 16) && q.x < end - 8) {
                c = nb(c, vec4(1.0), f32((end - 8 - q.x) * 127 / 8));
            }
            if (q.x >= max(start, end - 8) && q.x < end) {
                c = nb(c, vec4(0.0, 0.0, 0.0, 1.0), f32((q.x - end) * 255 / 8 + 255));
            }
        }
        return c;
    }

    if (k == 5) {
        let phase = i32(v.x);
        if (phase == 0) {
            return a;
        }
        if (phase == w / 2) {
            return b;
        }
        let mp = min(phase, 16);
        let shade = i32(sqrt(f32(max(0, (((w / 2 - phase) << 16) / w) / 2))) + 0.5);
        let hg = shade / 2;
        let x = q.x;
        var f = 0;
        var sc = 0;
        var mode = 0;
        if (v.y == 0.0) {
            let b1 = clamp(w - 2 * phase - mp, 0, w);
            let b2 = clamp(w - 2 * phase, 0, w);
            let b3 = clamp(w - 3 * phase / 2, 0, w);
            let b4 = clamp(w - phase, 0, w);
            if (x < b1) {
                return a;
            }
            if (x < b2) {
                return dark(a, ((x - b2 + mp) * shade / mp) / 2);
            }
            if (x < b3) {
                f = (b3 - x) * shade * 2 / phase;
                mode = 1;
            } else if (x < b4) {
                f = ((x - b4) * 2 + phase) * shade / phase;
                mode = 2;
            } else {
                return dark(b, (w - x) * shade / phase);
            }
            sc = x + 2 * phase - w;
        } else {
            let b1 = clamp(phase, 0, w);
            let b2 = clamp(3 * phase / 2, 0, w);
            let b3 = clamp(2 * phase, 0, w);
            let b4 = clamp(2 * phase + mp, 0, w);
            if (x < b1) {
                return dark(b, (b1 - x) * shade / phase);
            }
            if (x < b2) {
                f = (b2 - x) * shade * 2 / phase;
                mode = 2;
            } else if (x < b3) {
                f = (phase / 2 + (x - b3) * 2) * shade / phase;
                mode = 1;
            } else if (x < b4) {
                return dark(a, ((b4 - x) * shade / mp) / 2);
            } else {
                return a;
            }
            sc = x + w - 2 * phase;
        }
        var c = vec4(0.0);
        if (sc >= 0 && sc < w) {
            c = ns(vec2(sc, q.y), 1);
        }
        var top = dark(c, max(0, f));
        if (mode == 1) {
            top = light(c, max(0, f));
        }
        return vec4(nb(dark(a, hg), top, round(c.a * 255.0)).rgb, a.a);
    }

    if (k == 6) {
        let boundary = i32(p[26u + u32(q.y)].x);
        let threshold = i32(v.x);
        let left = select(b, a, v.y != 0.0);
        let right = select(a, b, v.y != 0.0);
        if (q.x < threshold) {
            return left;
        }
        if (q.x < threshold + boundary) {
            let rp = fetch(rule, vec2(q.x - threshold, q.y));
            return nb(left, rp, round(rp.a * 255.0));
        }
        return right;
    }

    if (k == 7) {
        let size = max(v.y, 4.0);
        let root3 = 1.7320508075688772;
        let halfW = size * root3 * 0.5;
        let aq = (root3 / 3.0 * f32(q.x) - f32(q.y) / 3.0) / size;
        let ar = (2.0 / 3.0 * f32(q.y)) / size;
        var rx = floor(aq + 0.5);
        var ry = floor(-aq - ar + 0.5);
        var rz = floor(ar + 0.5);
        let pixel = u32(q.y * w + q.x);
        let axes = p[26u + pixel / 4u];
        let axis = u32(axes[pixel & 3u]);
        if (axis == 1u) {
            rx = -ry - rz;
        } else if (axis == 2u) {
            ry = -rx - rz;
        } else {
            rz = -rx - ry;
        }
        let cx = i32(size * root3 * (rx + rz * 0.5) + 0.5);
        let cy = i32(size * 1.5 * rz + 0.5);
        let ady = abs(f32(q.y) - size * 1.5 * rz);
        var hh = 0.0;
        if (ady <= size * 0.5) {
            hh = halfW;
        } else if (ady <= size) {
            hh = halfW * (size - ady) / (size * 0.5);
        }
        let half = max(i32(hh + 0.5), 1);
        let order = i32(z.y);
        let dir = i32(z.z);
        var odx = 0.0;
        var ody = 1.0;
        var radial = false;
        if (order == 1) {
            odx = -1.0;
        } else if (order == 2) {
            odx = 0.0;
        } else if (order == 3) {
            odx = 1.0;
        } else if (order == 4) {
            odx = -1.0;
            ody = 0.0;
        } else if (order == 5) {
            radial = true;
        } else if (order == 6) {
            odx = 1.0;
            ody = 0.0;
        } else if (order == 7) {
            odx = -1.0;
            ody = -1.0;
        } else if (order == 8) {
            odx = 0.0;
            ody = -1.0;
        } else if (order == 9) {
            odx = 1.0;
            ody = -1.0;
        }
        let nx = f32(cx) / max(f32(w - 1), 1.0);
        let ny = f32(cy) / max(f32(h - 1), 1.0);
        let naxis = max(select(0.0, 1.0, odx != 0.0) + select(0.0, 1.0, ody != 0.0), 1.0);
        var delayf = 0.0;
        if (radial) {
            let ex = (nx - 0.5) * 2.0;
            let ey = (ny - 0.5) * 2.0;
            delayf = min(length(vec2(ex, ey)) / 1.4142135623730951, 1.0);
        } else {
            let px = select(select(0.0, nx, odx > 0.0), 1.0 - nx, odx < 0.0);
            let py = select(select(0.0, ny, ody > 0.0), 1.0 - ny, ody < 0.0);
            delayf = (px + py) / naxis;
        }
        let delay = clamp(i32(delayf * 192.0 + 0.5), 0, 192);
        let lp = i32(v.x) - delay;
        if (lp <= 0) {
            return a;
        }
        if (lp >= 63) {
            return b;
        }
        var src = 0;
        var vis = 0.0;
        if (lp < 32) {
            vis = f32(32 - lp) / 32.0;
        } else {
            src = 1;
            vis = f32(lp - 31) / 32.0;
        }
        let visHalf = f32(half) * vis;
        if (visHalf < 0.5) {
            return vec4(0.0);
        }
        var rel = f32(q.x - cx);
        if (dir == 1 || dir == 4 || dir == 7) {
            rel = -rel;
        }
        if (rel < -visHalf || rel > visHalf) {
            return vec4(0.0);
        }
        let srcRel = rel * f32(half) / visHalf + f32(i32(z.x)) / max(size, 1.0) * f32(q.y - cy) * (1.0 - vis);
        let srcX = cx + i32(srcRel + select(-0.5, 0.5, srcRel >= 0.0));
        if (srcX < 0 || srcX >= w) {
            return vec4(0.0);
        }
        return ns(vec2(srcX, q.y), src);
    }

    if (k == 8) {
        let pixels = u32(z.x);
        let base = u32(q.y * w + q.x);
        let code_base = pixels * 3u;
        var c = vec4(0.0);
        if (v.y >= 0.0) {
            c = ns(q, i32(v.y));
        }
        let phase = i32(v.x);
        var node = atomicLoad(&records[base]);
        loop {
            if (node == 0u) {
                break;
            }
            let code = atomicLoad(&records[code_base + node - 1u]);
            let c0 = code - 1u;
            let src = i32(c0 / pixels);
            let index = i32(c0 % pixels);
            let sq = vec2(index % w, index / w);
            let rp = fetch(rule, sq);
            let start = i32(round(rp.b * 255.0));
            let localPhase = select(255 - phase, phase, src == 0);
            let sv = ns(sq, src);
            if (localPhase < start) {
                c = sv;
            } else {
                c = nb(c, sv, f32(max((localPhase - start) * 255 / max(256 - start, 1), 0)));
            }
            node = atomicLoad(&records[pixels + node - 1u]);
        }
        return c;
    }

    if (k == 9) {
        let count = i32(v.x);
        let base = i32(v.y);
        let roundness = max(z.x, 0.000001);
        let len = i32(z.y);
        var blend = base;
        var vd = 0;
        for (var i = 0; i < count; i++) {
            let r = p[26u + u32(i) * 2u];
            if (i32(r.w) != 1) {
                continue;
            }
            let dx = abs(q.x - i32(r.x));
            let dy = abs(q.y - i32(r.y));
            let distance = min(i32(floor(sqrt(f32(dx * dx) + f32(dy * dy) * roundness * roundness) + 0.5)), 65535);
            var wc = i32(r.z) - distance;
            if (wc >= 0) {
                wc = min(wc, len - 1);
                let table = p[26u + u32(count) * 2u + u32(wc)];
                let drift = p[26u + u32(i) * 2u + 1u].x;
                blend += i32(table.y);
                vd -= (i32(table.x) * i32(drift)) >> 8;
            }
        }
        let aa = ns(vec2(q.x, clamp(q.y + vd, 0, h - 1)), 0);
        let bb = ns(vec2(q.x, clamp(q.y + vd, 0, h - 1)), 1);
        if ((blend & 256) != 0) {
            return vec4(nb(bb, aa, f32(blend & 255)).rgb, 1.0);
        }
        return vec4(nb(aa, bb, f32(blend & 255)).rgb, 1.0);
    }

    if (k == 10) {
        let triangleCount = i32(p[3].w);
        var mappedA = q;
        var mappedB = q;
        for (var tri = 0; tri < triangleCount; tri++) {
            let j = 26u + u32(tri) * 6u;
            let a = vec2<i32>(p[j].xy);
            let b = vec2<i32>(p[j + 2u].xy);
            let c = vec2<i32>(p[j + 4u].xy);
            let den = (b.y - c.y) * (a.x - c.x) + (c.x - b.x) * (a.y - c.y);
            if (den == 0) {
                continue;
            }
            let w0 = (b.y - c.y) * (q.x - c.x) + (c.x - b.x) * (q.y - c.y);
            let w1 = (c.y - a.y) * (q.x - c.x) + (a.x - c.x) * (q.y - c.y);
            let w2 = den - w0 - w1;
            if (den > 0) {
                if (w0 < 0 || w1 < 0 || w2 < 0) {
                    continue;
                }
            } else if (w0 > 0 || w1 > 0 || w2 > 0) {
                continue;
            }
            let fw0 = f32(w0) / f32(den);
            let fw1 = f32(w1) / f32(den);
            let fw2 = f32(w2) / f32(den);
            let ba = p[j].zw;
            let bb = p[j + 2u].zw;
            let bc = p[j + 4u].zw;
            let da = p[j + 1u].xy;
            let db = p[j + 3u].xy;
            let dc = p[j + 5u].xy;
            mappedA = clamp(vec2<i32>(floor(fw0 * ba + fw1 * bb + fw2 * bc + 0.5)), vec2(0), vec2(w - 1, h - 1));
            mappedB = clamp(vec2<i32>(floor(fw0 * da + fw1 * db + fw2 * dc + 0.5)), vec2(0), vec2(w - 1, h - 1));
        }
        return nb(ns(mappedA, 0), ns(mappedB, 1), f32(v.x));
    }

    return a;
}

fn nagano_record(destination: u32, code: u32, pixels: u32) {
    let next_base = pixels;
    let code_base = pixels * 3u;
    let counter = pixels * 5u;
    let node = atomicAdd(&records[counter], 1u) + 1u;
    atomicStore(&records[code_base + node - 1u], code);
    loop {
        let first = atomicLoad(&records[destination]);
        if (first == 0u || atomicLoad(&records[code_base + first - 1u]) > code) {
            atomicStore(&records[next_base + node - 1u], first);
            let inserted = atomicCompareExchangeWeak(&records[destination], first, node);
            if (inserted.exchanged) {
                return;
            }
            continue;
        }
        var previous = first;
        var following = atomicLoad(&records[next_base + previous - 1u]);
        while (following != 0u && atomicLoad(&records[code_base + following - 1u]) < code) {
            previous = following;
            following = atomicLoad(&records[next_base + previous - 1u]);
        }
        atomicStore(&records[next_base + node - 1u], following);
        let inserted = atomicCompareExchangeWeak(&records[next_base + previous - 1u], following, node);
        if (inserted.exchanged) {
            return;
        }
    }
}

@compute @workgroup_size(8, 8)
fn nagano_scatter(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let q = vec2<i32>(invocation.xy);
    let w = i32(p[3].x);
    let h = i32(p[3].y);
    let phase = i32(p[3].z);
    let mask = u32(p[4].y);
    if (any(q >= vec2(w, h))) {
        return;
    }
    let rp = fetch(rule, q);
    let start = i32(round(rp.b * 255.0));
    let dir = i32(round(rp.r * 255.0));
    let spd = i32(round(rp.g * 255.0));
    let trig = nagano_tables[u32(dir)].xy;
    let pixels = u32(p[4].x);
    let index = u32(q.y * w + q.x);
    for (var step = 0u; step < 2u; step++) {
        if ((mask & (1u << step)) == 0u) {
            continue;
        }
        let ph = select(255 - phase, phase, step == 0u);
        var nx = q.x;
        var ny = q.y;
        if (ph >= start) {
            let amount = i32(nagano_tables[nagano_motion + u32(ph - start)][step]);
            nx += (i32(trig.x) * spd * amount) >> 19;
            ny += (i32(trig.y) * spd * amount) >> 19;
        }
        if (nx < 0 || nx >= w || ny < 0 || ny >= h) {
            continue;
        }
        nagano_record(u32(ny * w + nx), step * pixels + index + 1u, pixels);
    }
}
