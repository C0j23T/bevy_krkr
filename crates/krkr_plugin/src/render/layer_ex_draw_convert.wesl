@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var previous: texture_2d<f32>;
@group(0) @binding(2) var output: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(3) var<uniform> settings: vec4<u32>;
@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let q = vec2<i32>(gid.xy);
    if (any(gid.xy >= textureDimensions(output))) { return; }
    let s = textureLoad(source, q, 0);
    if (settings.x == 3u) {
        textureStore(output, q, vec4(s.rgb * s.a, s.a));
        return;
    }
    let d = textureLoad(previous, q, 0);
    if (s.a == 0.0) { textureStore(output, q, d); return; }
    let a = s.a + d.a * (1.0 - s.a);
    var result = vec4(0.0);
    if (settings.x == 0u && a > 0.0) {
        result = vec4((s.rgb + d.rgb * d.a * (1.0 - s.a)) / a, a);
    } else if (settings.x == 1u) {
        result = vec4(s.rgb + d.rgb * (1.0 - s.a), a);
    } else if (settings.x == 2u) {
        result = vec4(s.rgb + d.rgb * (1.0 - s.a), 1.0);
    }
    textureStore(output, q, clamp(result, vec4(0.0), vec4(1.0)));
}
