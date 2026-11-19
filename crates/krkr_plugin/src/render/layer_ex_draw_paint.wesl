@group(0) @binding(0) var<storage, read> data: array<vec4<f32>>;
@group(0) @binding(1) var output: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(2) var source: texture_2d<f32>;

fn extend_axis(value: f32, origin: f32, size: f32, reflect: bool) -> f32 {
    if size <= 0.0 { return value; }
    let t = (value - origin) / size;
    if reflect { return origin + (1.0 - abs(fract(t * 0.5) * 2.0 - 1.0)) * size; }
    return origin + fract(t) * size;
}

fn linear_rgb(c: vec3<f32>) -> vec3<f32> {
    return select(pow((c + 0.055) / 1.055, vec3(2.4)), c / 12.92, c <= vec3(0.04045));
}

fn interpolate(a: vec4<f32>, b: vec4<f32>, factor: f32) -> vec4<f32> {
    if data[7].w == 0.0 { return mix(a, b, factor); }
    let v = mix(linear_rgb(a.rgb), linear_rgb(b.rgb), factor);
    let rgb = select(1.055 * pow(v, vec3(1.0 / 2.4)) - 0.055, 12.92 * v, v <= vec3(0.0031308));
    return vec4(rgb, mix(a.a, b.a, factor));
}

fn ramp(t: f32) -> vec4<f32> {
    let count = u32(data[0].w);
    var a = data[9];
    var start = data[10].x;
    for (var i = 1u; i < count; i++) {
        let b = data[9u + 2u * i];
        let end = data[10u + 2u * i].x;
        if t <= end || i + 1u == count {
            let fraction = clamp((t - start) / max(end - start, 0.000001), 0.0, 1.0);
            if data[7].z != 0.0 { return interpolate(a, b, fraction); }
            return mix(a, b, fraction);
        }
        a = b;
        start = end;
    }
    return a;
}

fn cross2(a: vec2<f32>, b: vec2<f32>) -> f32 { return a.x * b.y - a.y * b.x; }

fn path_gradient(q: vec2<f32>) -> vec4<f32> {
    let center = data[4].xy;
    let ray = q - center;
    var outer = 1e20;
    var inner = 0.0;
    var edge = vec4(0.0);
    var inside = false;
    let count = u32(data[5].z);
    let base = 9u + 2u * u32(data[0].w);
    for (var i = 0u; i < count; i++) {
        let j = (i + 1u) % count;
        let a = data[base + 2u * i].xy;
        let b = data[base + 2u * j].xy;
        let segment = b - a;
        if (a.y > q.y) != (b.y > q.y) {
            if q.x < a.x + (q.y - a.y) * segment.x / segment.y { inside = !inside; }
        }
        let det = cross2(ray, segment);
        if abs(det) > 0.000001 {
            let t = cross2(a - center, segment) / det;
            let u = cross2(a - center, ray) / det;
            if t >= 1.0 && u >= 0.0 && u <= 1.0 && t < outer {
                outer = t;
                edge = interpolate(data[base + 2u * i + 1u], data[base + 2u * j + 1u], u);
            }
        }
        let scaled = (a - center) * data[5].xy;
        let scaled_edge = segment * data[5].xy;
        let scaled_det = cross2(ray, scaled_edge);
        if abs(scaled_det) > 0.000001 {
            let t = cross2(scaled, scaled_edge) / scaled_det;
            let u = cross2(scaled, ray) / scaled_det;
            if t > 0.0 && u >= 0.0 && u <= 1.0 { inner = max(inner, t); }
        }
    }
    if !inside { return vec4(0.0); }
    var t = 1.0;
    if dot(ray, ray) > 0.000001 {
        t = 1.0 - clamp((1.0 - inner) / max(outer - inner, 0.000001), 0.0, 1.0);
    }
    let blend = ramp(t);
    if data[7].z != 0.0 { return blend; }
    return interpolate(edge, data[6], blend.r);
}

fn source_pixel(p: vec2<i32>) -> vec4<f32> {
    if any(p < vec2(0)) || any(p >= vec2<i32>(textureDimensions(source))) { return vec4(0.0); }
    let c = textureLoad(source, p, 0);
    return vec4(c.rgb * c.a, c.a);
}

fn cropped_image(q: vec2<f32>, tile: vec4<f32>) -> vec4<f32> {
    let inset = min(vec2(0.5), tile.zw * 0.5);
    let uv = clamp(q, tile.xy + inset, tile.xy + tile.zw - inset) - 0.5;
    let p = vec2<i32>(floor(uv));
    let f = fract(uv);
    return mix(mix(source_pixel(p), source_pixel(p + vec2(1, 0)), f.x),
               mix(source_pixel(p + vec2(0, 1)), source_pixel(p + vec2(1, 1)), f.x), f.y);
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if any(gid.xy >= textureDimensions(output)) { return; }
    let pos = vec2<f32>(gid.xy) + 0.5;
    var q = vec2(dot(data[1].xy, pos) + data[1].z, dot(data[2].xy, pos) + data[2].z);
    let tile = data[3];
    let wrap = u32(data[0].z);
    if wrap < 4u {
        q.x = extend_axis(q.x, tile.x, tile.z, wrap == 1u || wrap == 3u);
        q.y = extend_axis(q.y, tile.y, tile.w, wrap == 2u || wrap == 3u);
    }
    if data[0].y == 2.0 {
        textureStore(output, vec2<i32>(gid.xy), cropped_image(q, tile));
        return;
    }
    var c: vec4<f32>;
    if data[0].y == 4.0 {
        let axis = data[4].zw - data[4].xy;
        let blend = ramp(clamp(dot(q - data[4].xy, axis) / dot(axis, axis), 0.0, 1.0));
        if data[7].z != 0.0 { c = blend; }
        else { c = interpolate(data[6], data[8], blend.r); }
    } else {
        c = path_gradient(q);
    }
    textureStore(output, vec2<i32>(gid.xy), vec4(c.rgb * c.a, c.a));
}
