fn mesh_basis(t: f32) -> vec4<f32> {
    let s = 1.0 - t;
    return vec4(s*s*s, 3.0*t*s*s, 3.0*t*t*s, t*t*t);
}

fn mesh_patch(base: u32, uv: vec2<f32>) -> vec2<f32> {
    let a = mesh_basis(uv.x);
    let b = mesh_basis(uv.y);
    var point = vec2(0.0);
    for (var row = 0u; row < 4u; row++) {
        for (var col = 0u; col < 4u; col++) {
            let index = row * 4u + col;
            let pair = p[base + index / 2u];
            point += select(pair.xy, pair.zw, index % 2u == 1u) * a[row] * b[col];
        }
    }
    return point;
}

@compute @workgroup_size(64)
fn deform_mesh(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let divisions = vec2<u32>(p[2].yz);
    let vertex = invocation.x;
    if (vertex >= (divisions.x + 1u) * (divisions.y + 1u)) { return; }
    let uv = vec2<f32>(f32(vertex % (divisions.x + 1u)) / f32(divisions.x), f32(vertex / (divisions.x + 1u)) / f32(divisions.y));
    var points: array<vec4<f32>, 128>;
    var masks: array<u32, 128>;
    var count = 0u;
    var geometry_mask = 0x1f0u;
    var trans = vec4(0.0);
    let surface_count = u32(p[2].x);
    for (var index = i32(surface_count) - 1; index >= 0; index--) {
        let base = 6u + u32(index) * 140u;
        let geometry = p[base + 137u];
        let offset = p[base + 138u];
        let tags = bitcast<vec4<u32>>(p[base + 139u]);
        let innermost = u32(index) + 1u == surface_count;
        if (innermost || tags.x == 1u) {
            var point = uv;
            if (tags.x != 3u) {
                var at = uv;
                if (!innermost) { at = (trans.yx + geometry.yx) / geometry.wz; }
                point = at.yx;
                if (tags.x == 1u) { point = mesh_patch(base, at); }
                point = point * geometry.zw - geometry.xy - offset.xy;
            }
            points[count] = vec4(point, 0.0, 0.0) - trans;
            masks[count] = geometry_mask;
            count++;
        }
        trans = vec4(0.0);
        for (var j = 0u; j < count; j++) {
            let mask = select(masks[j], 0x1f0u, index == 0 || innermost);
            let matrix = base + 8u + (mask >> 4u) * 4u;
            let model = mat4x4(p[matrix], p[matrix + 1u], p[matrix + 2u], p[matrix + 3u]);
            points[j] = model * points[j];
            masks[j] &= tags.y;
            trans += points[j];
        }
        let placement = p[base + 136u];
        if (count > 0u && masks[count - 1u] == 0x1f0u) {
            points[count - 1u] += placement;
        } else {
            points[count] = placement;
            masks[count] = 0x1f0u;
            count++;
        }
        trans += placement;
        geometry_mask &= tags.y;
    }
    let result = bitcast<vec4<u32>>(vec4(trans.x, -trans.y, uv.y, uv.x));
    for (var component = 0u; component < 4u; component++) {
        atomicStore(&records[vertex * 4u + component], result[component]);
    }
}
