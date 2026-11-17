@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var mask: texture_2d<f32>;
@group(0) @binding(2) var previous: texture_2d<f32>;
@group(0) @binding(3) var image_sampler: sampler;
@group(0) @binding(4) var<uniform> settings: array<vec4<f32>, 4>;

struct Vertex {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn mesh_vertex(@location(0) point: vec4<f32>) -> Vertex {
    var result: Vertex;
    result.position = vec4(point.x, -point.y, 0.0, 1.0);
    result.uv = point.zw;
    return result;
}

@vertex
fn prepare_vertex(@builtin(vertex_index) index: u32) -> Vertex {
    let xy = vec2(f32((index << 1u) & 2u), f32(index & 2u));
    var result: Vertex;
    result.position = vec4(xy * 2.0 - 1.0, 0.0, 1.0);
    result.uv = xy;
    return result;
}

@fragment
fn prepare_fragment(vertex: Vertex) -> @location(0) vec4<f32> {
    let color = textureLoad(previous, vec2<i32>(vertex.position.xy), 0);
    if (settings[0].y == 0.0 || settings[0].y == 21.0) {
        return vec4(color.rgb * color.a, color.a);
    }
    return color;
}

@fragment
fn mesh_fragment(vertex: Vertex) -> @location(0) vec4<f32> {
    if (settings[0].z != 0.0) {
        let uv = vertex.position.xy / settings[3].xy;
        if (textureSampleLevel(mask, image_sampler, uv, 0.0).a < 0.5) { discard; }
    }
    var color = textureSampleLevel(source, image_sampler, vertex.uv, 0.0);
    if (settings[0].y == 21.0) { color = vec4(settings[2].rgb, settings[2].a * color.a); }
    color *= settings[1];
    color.a *= settings[0].x;
    color = clamp(color, vec4(0.0), vec4(1.0));
    if (settings[0].y == 0.0 || settings[0].y == 21.0) { color = vec4(color.rgb * color.a, color.a); }
    return color;
}
