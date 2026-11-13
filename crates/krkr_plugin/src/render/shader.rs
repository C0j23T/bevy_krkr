use std::sync::LazyLock;

use bevy::platform::collections::HashMap;
use indoc::indoc;
use itertools::Itertools;

const IMPORT: &str = "import package::krkr::extNagano::nagano;";

static COMPOSER: LazyLock<ShaderComposer> = LazyLock::new(|| {
    let mut composer = ShaderComposer::new();
    add_operations(&mut composer);
    composer
});

pub fn compose() -> String {
    COMPOSER.compose()
}

struct Function<'a> {
    name: &'a str,
    params: Vec<(&'a str, &'a str)>,
    ret: &'a str,
    body: &'a str,
}

impl<'a> Function<'a> {
    fn compose(&self) -> String {
        format!(
            "fn {}({})->{}{{{}}}",
            self.name,
            self.params
                .iter()
                .map(|(name, ty)| format!("{name}: {ty}"))
                .join(","),
            self.ret,
            self.body,
        )
    }
}

struct ShaderComposer<'a> {
    bindings: HashMap<u32, (&'a str, Option<&'a str>, &'a str)>,
    funcs: Vec<Function<'a>>,
    operations: HashMap<u32, String>,
}

fn indent(body: &str, spaces: usize) -> String {
    let prefix = " ".repeat(spaces);
    body.lines().map(|line| format!("{prefix}{line}\n")).collect()
}

fn builtin_funcs() -> Vec<Function<'static>> {
    vec![
        Function {
            name: "edge",
            params: vec![("a", "vec2<f32>"), ("b", "vec2<f32>"), ("v", "vec2<f32>")],
            ret: "f32",
            body: "return (b.x - a.x) * (v.y - a.y) - (b.y - a.y) * (v.x - a.x);",
        },
        Function {
            name: "inside",
            params: vec![("at", "vec2<f32>")],
            ret: "bool",
            body: indoc! {"
                if (p[1].w == 0.0) {
                    return true;
                }

                let a = p[9].xy;
                let b = p[9].zw;
                let c = p[10].xy;

                let ab = edge(a,b,at);
                let bc = edge(b,c,at);
                let ca = edge(c,a,at);

                if (p[1].w == 3.0) {
                    let winding = sign(edge(a, b, c));
                    let edges = vec3(ab, bc, ca) * winding;
                    let delta = array<vec2<f32>, 3>((b - a) * winding, (c - b) * winding, (a - c) * winding);
                    for (var i = 0u; i<3u; i++) {
                        if (edges[i] < 0.0) {
                            return false;
                        }
                        if (edges[i] == 0.0 && !(delta[i].y < 0.0 || (delta[i].y == 0.0 && delta[i].x > 0.0))) {
                            return false;
                        }
                    }
                    return true;
                }
                let d = p[10].zw;
                let e = vec4(edge(a, b, at), edge(b, d, at), edge(d, c, at), edge(c, a, at));
                return all(e >= vec4(0)) || all(e <= vec4(0));
            "},
        },
        Function {
            name: "coordinate",
            params: vec![("i", "u32"), ("at", "vec2<f32>")],
            ret: "vec2<f32>",
            body: indoc! {"
                if (p[1].w == 0.0) {
                    return mix(p[6 + i].xy, p[6 + i].zw, (at - p[0].xy) / (p[0].zw - p[0].xy));
                }
                let j = 17 + i * 3;
                let q = vec3(dot(p[j].xyz, vec3(at, 1)), dot(p[j + 1].xyz, vec3(at, 1)), dot(p[j + 2].xyz, vec3(at, 1)));
                return q.xy / q.z;
            "},
        },
        Function {
            name: "fetch",
            params: vec![("t", "texture_2d<f32>"), ("at", "vec2<i32>")],
            ret: "vec4<f32>",
            body: "return textureLoad(t, clamp(at, vec2(0), vec2<i32>(textureDimensions(t)) - 1), 0);",
        },
        Function {
            name: "source_fetch",
            params: vec![("t", "texture_2d<f32>"), ("at", "vec2<i32>")],
            ret: "vec4<f32>",
            body: indoc! {"
                if (p[1].w != 0.0) {
                    return fetch(t,at);
                }
                let low = vec2<i32>(floor(min(p[6].xy, p[6].zw)));
                let high = vec2<i32>(ceil(max(p[6].xy, p[6].zw))) - 1;
                return fetch(t, clamp(at, low, high));
            "},
        },
        Function {
            name: "sinc",
            params: vec![("x", "f32")],
            ret: "f32",
            body: indoc! {"
                if (abs(x) < 0.000001) {
                    return 1.0;
                }
                return sin(3.14159265359 * x) / (3.14159265359 * x);
            "},
        },
        Function {
            name: "weight",
            params: vec![("distance", "f32"), ("mode", "u32")],
            ret: "f32",
            body: indoc! {"
                let x = abs(distance);
                switch mode {
                    case 3u, 5u: {
                        if (x <= 1.0) {
                            return 1.0 - 2.0 * x * x + x * x * x;
                        }
                        if (x < 2.0) {
                            return 4.0 - 8.0 * x + 5.0 * x * x - x * x * x;
                        }
                    }
                    case 6u,7u: {
                        if (x < 2.0) {
                            return sinc(x) * sinc(x / 2.0);
                        }
                    }
                    case 8u,9u: {
                        if (x < 3.0) {
                            return sinc(x) * sinc(x / 3.0);
                        }
                    }
                    case 10u,11u: {
                        if (x <= 1.0) {
                            return x * x * x - 1.8 * x * x - 0.2 * x + 1.0;
                        }
                        if (x < 2.0) {
                            return -x * x * x / 3.0 + 1.8 * x * x - 46.0 * x / 15.0 + 1.6;
                        }
                    }
                    case 12u,13u: {
                        if (x <= 1.0) {
                            return 13.0 * x * x * x / 11.0 - 453.0 * x * x / 209.0 - 3.0 * x / 209.0 + 1.0;
                        }
                        if (x <= 2.0) {
                            return -6.0 * x * x * x / 11.0 + 612.0 * x * x / 209.0 - 1038.0 * x / 209.0 + 540.0 / 209.0;
                        }
                        if (x < 3.0) {
                            return x * x * x / 11.0 - 159.0 * x * x / 209.0 + 434.0 * x / 209.0 - 384.0 / 209.0;
                        }
                    }
                    case 16u,17u: {
                        if (x < 2.0) {
                            return exp(-2.0 * x * x);
                        }
                    }
                    case 18u,19u: {
                        if (x < 4.0) {
                            return sinc(x) * (0.42 + 0.5 * cos(3.14159265359 * x / 4.0) + 0.08 * cos(3.14159265359 * x / 2.0));
                        }
                    }
                    default: {
                        return max(1.0 - x, 0.0);
                    }
                }
                return 0.0;
            "},
        },
        Function {
            name: "sample",
            params: vec![("t", "texture_2d<f32>"), ("at", "vec2<f32>")],
            ret: "vec4<f32>",
            body: indoc! {"
                let mode = u32(p[1].z);

                if (mode==0u) {
                    return source_fetch(t, vec2<i32>(floor(at)));
                }

                let q = at - 0.5;
                let a = vec2<i32>(floor(q));
                let f = fract(q);
                if (mode == 1u || mode == 2u || mode == 4u) {
                    return mix(
                        mix(source_fetch(t, a), source_fetch(t, a + vec2(1, 0)), f.x),
                        mix(source_fetch(t, a + vec2(0, 1)), source_fetch(t, a + vec2(1, 1)), f.x),
                        f.y
                    );
                }

                var scale = vec2(1.0);
                if (p[1].w == 0.0) {
                    scale = max(abs((p[6].zw - p[6].xy) / (p[0].zw - p[0].xy)), vec2(1));
                }

                var radius = 2.0;
                if (mode == 8u || mode == 9u || mode == 12u || mode == 13u) {
                    radius = 3.0;
                }
                if (mode == 18u || mode == 19u) {
                    radius = 4.0;
                }
                let area = mode == 14u || mode == 15u;
                if (area) {
                    radius = 0.5;
                }

                var sum = vec4(0.0);
                var total = 0.0;
                let first = vec2<i32>(floor(q - radius * scale));
                let last = vec2<i32>(ceil(q + radius * scale));
                for (var y = first.y; y <= last.y; y++) {
                    for (var x = first.x; x <= last.x; x++) {
                        let distance = vec2<f32>(f32(x), f32(y)) - q;
                        var contribution = weight(distance.x / scale.x, mode) * weight(distance.y / scale.y, mode);
                        if (area) {
                            let overlap = max(
                                min(vec2<f32>(f32(x), f32(y)) + 0.5, q + scale * 0.5)
                                - max(vec2<f32>(f32(x), f32(y)) - 0.5, q - scale * 0.5),
                                vec2(0),
                            );
                            contribution = overlap.x * overlap.y;
                        }
                        sum += source_fetch(t, vec2(x, y)) * contribution;
                        total += contribution;
                    }
                }
                return sum / max(total, 0.000001);
            "},
        },
        Function {
            name: "ns",
            params: vec![("q", "vec2<i32>"), ("which", "i32")],
            ret: "vec4<f32>",
            body: indoc! {"
                if (which == 0) {
                    return fetch(source, q + vec2<i32>(p[6].xy));
                }
                return fetch(destination, q + vec2<i32>(p[7].xy));
            "},
        },
        Function {
            name: "blend_channel",
            params: vec![
                ("a", "u32"),
                ("b", "u32"),
                ("opa", "u32"),
                ("shift", "u32"),
            ],
            ret: "u32",
            body: indoc! {"
                if (shift == 24u) {
                    return (a + (((b - a) * opa) >> 8u)) & 255u;
                }
                let x = a << shift;
                let y = b << shift;
                return ((x + (((y - x) * opa) >> 8u)) >> shift) & 255u;
            "},
        },
        Function {
            name: "nb",
            params: vec![("a", "vec4<f32>"), ("b", "vec4<f32>"), ("f", "f32")],
            ret: "vec4<f32>",
            body: indoc! {"
                let aa = vec4<u32>(round(a * 255.0));
                let bb = vec4<u32>(round(b * 255.0));
                let opa = u32(max(f, 0.0));
                return vec4<f32>(
                    f32(blend_channel(aa.r, bb.r, opa, 16u)),
                    f32(blend_channel(aa.g, bb.g, opa, 8u)),
                    f32(blend_channel(aa.b, bb.b, opa, 0u)),
                    f32(blend_channel(aa.a, bb.a, opa, 24u)),
                ) / 255.0;
            "},
        },
    ]
}

impl<'a> ShaderComposer<'a> {
    fn new() -> Self {
        let mut output = Self {
            bindings: HashMap::new(),
            funcs: Vec::new(),
            operations: HashMap::new(),
        };

        // @group(0) @binding(0) public var source: texture_2d<f32>;
        output
            .bindings
            .insert(0, ("source", None, "texture_2d<f32>"));
        // @group(0) @binding(1) public var destination: texture_2d<f32>;
        output
            .bindings
            .insert(1, ("destination", None, "texture_2d<f32>"));
        // @group(0) @binding(2) public var rule: texture_2d<f32>;
        output.bindings.insert(2, ("rule", None, "texture_2d<f32>"));
        // @group(0) @binding(3) public var previous: texture_2d<f32>;
        output
            .bindings
            .insert(3, ("previous", None, "texture_2d<f32>"));
        // @group(0) @binding(4) public var output: texture_storage_2d<rgba8unorm, write>;
        output
            .bindings
            .insert(4, ("output", None, "texture_storage_2d<rgba8unorm, write>"));
        // @group(0) @binding(5) public var<storage, read> p: array<vec4<f32>>;
        output
            .bindings
            .insert(5, ("p", Some("storage, read"), "array<vec4<f32>>"));
        // @group(0) @binding(6) public var<storage, read_write> records: array<atomic<u32>>;
        output.bindings.insert(
            6,
            ("records", Some("storage, read_write"), "array<atomic<u32>>"),
        );
        // @group(0) @binding(7) public var<storage, read> nagano_tables: array<vec4<f32>>;
        output.bindings.insert(
            7,
            ("nagano_tables", Some("storage, read"), "array<vec4<f32>>"),
        );

        output.funcs.extend(builtin_funcs());

        output
    }

    fn compose(&self) -> String {
        let mut output = String::new();

        output.push_str(IMPORT);
        output.push_str("\n\n");
        self.bindings
            .iter()
            .sorted_by_key(|(binding, _)| **binding)
            .for_each(|(binding, param)| {
                output.push_str(&format!(
                    "@group(0) @binding({binding}) public var{} {}: {};\n",
                    param
                        .1
                        .map(|storage| format!("<{storage}>"))
                        .unwrap_or_default(),
                    param.0,
                    param.2,
                ));
            });
        output.push('\n');
        self.funcs.iter().for_each(|func| {
            output.push_str(&func.compose());
            output.push('\n');
        });

        let cases = self
            .operations
            .iter()
            .sorted_by_key(|(opcode, _)| **opcode)
            .map(|(opcode, body)| {
                format!(
                    "        case {opcode}u: {{\n{}            break;\n        }}\n",
                    indent(body, 12)
                )
            })
            .collect::<String>();

        output.push_str(&indoc! {"
            @compute @workgroup_size(8,8)
            fn main(@builtin(global_invocation_id) invocation: vec3<u32>) {
                let xy = vec2<i32>(invocation.xy) + vec2<i32>(max(p[0].xy, vec2(0)));
                let at = vec2<f32>(xy) + 0.5; // pixel center
                if any(xy >= vec2<i32>(textureDimensions(output))) || any(at >= p[0].zw) || !inside(at) {
                    return;
                }

                let operation = u32(p[1].x);
                switch operation {
        "});
        output.push_str(&cases);
        output.push_str(&indoc! {"
                    default: {
                        textureStore(output, xy, fetch(previous, xy));
                        break;
                    }
                }
            }
        "});

        output
    }

    fn add_operation(&mut self, opcode: u32, body: impl AsRef<str>) {
        assert!(
            self.operations
                .insert(opcode, body.as_ref().to_string())
                .is_none(),
            "duplicate opcode {opcode}"
        );
    }
}

fn add_operations(composer: &mut ShaderComposer) {
    krkr_operations(composer);
    layerex_operations(composer);
    extrans_operations(composer);
    nagano_operations(composer);
}

fn krkr_operations(composer: &mut ShaderComposer) {
    composer.funcs.push(Function {
        name: "blend_color",
        params: vec![("d", "vec3<f32>"), ("s", "vec3<f32>"), ("mode", "u32")],
        ret: "vec3<f32>",
        body: indoc! {"
            switch mode {
                case 1u: {
                    return min(d + s, vec3(1));
                }
                case 2u: {
                    return max(d + s - 1.0, vec3(0));
                }
                case 3u: {
                    return d * s;
                }
                case 4u: {
                    return 1.0 - (1.0 - d) * (1.0 - s);
                }
                case 5u: {
                    return select(2.0 * d * s, 1.0 - 2.0 * (1.0 - d) * (1.0 - s), d > vec3(0.5));
                }
                case 6u: {
                    return select(2.0 * d * s, 1.0 - 2.0 * (1.0 - d) * (1.0 - s), s > vec3(0.5));
                }
                case 7u: {
                    return select(d - (1.0 - 2.0 * s) * d * (1.0 - d), d + (2.0 * s - 1.0) * (sqrt(d) - d), s > vec3(0.5));
                }
                case 8u: {
                    return min(d / max(1.0 - s, vec3(0.000001)), vec3(1));
                }
                case 9u: {
                    return 1.0 - min((1.0 - d) / max(s, vec3(0.000001)), vec3(1));
                }
                case 10u: {
                    return max(d, s);
                }
                case 11u: {
                    return min(d, s);
                }
                case 12u: {
                    return abs(d - s);
                }
                case 13u: {
                    return d + s - 2.0 * d * s;
                }
                default: {
                    return s;
                }
            }
        "},
    });
    composer.funcs.push(Function {
        name: "over",
        params: vec![
            ("d", "vec4<f32>"),
            ("s", "vec4<f32>"),
            ("opacity", "f32"),
            ("kind", "u32"),
        ],
        ret: "vec4<f32>",
        body: indoc! {"
            let alpha = s.a * opacity;
            let out_alpha = alpha + d.a * (1.0 - alpha);
            if (kind == 1u) {
                return vec4((s.rgb * alpha + d.rgb * d.a * (1.0 - alpha)) / max(out_alpha, 0.000001), out_alpha);
            }
            if (kind == 2u) {
                return vec4(s.rgb * alpha + d.rgb * (1.0 - alpha), out_alpha);
            }
            if (kind == 3u) {
                return vec4(s.rgb * opacity + d.rgb * (1.0 - alpha), d.a);
            }
            if (kind == 4u) {
                return vec4(s.rgb * opacity + d.rgb * (1.0 - alpha), out_alpha);
            }
            if (kind == 5u) {
                return vec4((s.rgb * opacity + d.rgb * d.a * (1.0 - alpha)) / max(out_alpha, 0.000001), out_alpha);
            }
            return vec4(mix(d.rgb, s.rgb, alpha), d.a);
        "},
    });
    composer.funcs.push(Function {
        name: "kirikiri_dodge",
        params: vec![
            ("d", "vec3<f32>"),
            ("s", "vec3<f32>"),
            ("opacity", "u32"),
        ],
        ret: "vec3<f32>",
        body: indoc! {"
            if (opacity == 0u) {
                return d;
            }
            var source_bytes = vec3<u32>(round(clamp(s, vec3(0), vec3(1)) * 255.0));
            if (opacity < 255u) {
                source_bytes = (source_bytes * opacity) >> vec3(8u);
            }
            let destination_bytes = vec3<u32>(round(clamp(d, vec3(0), vec3(1)) * 255.0));
            let reciprocal = vec3(65536u) / max(vec3(255u) - source_bytes, vec3(1u));
            return vec3<f32>(min((destination_bytes * reciprocal) >> vec3(8u), vec3(255u))) / 255.0;
        "},
    });

    // Copy
    composer.add_operation(
        0,
        indoc! {"
            let s = sample(source, coordinate(0u, at));
            textureStore(output, xy, clamp(s, vec4(0), vec4(1)));
        "},
    );

    // FillARGB
    composer.add_operation(
        1,
        indoc! {"
            textureStore(output, xy, clamp(p[5], vec4(0), vec4(1)));
        "},
    );

    // FillColor
    composer.add_operation(
        2,
        indoc! {"
            let d = fetch(destination, vec2<i32>(floor(coordinate(1u, at))));
            textureStore(output, xy, clamp(vec4(p[5].rgb, d.a), vec4(0), vec4(1)));
        "},
    );

    // FillMask
    composer.add_operation(
        3,
        indoc! {"
            let d = fetch(destination, vec2<i32>(floor(coordinate(1u, at))));
            let result = vec4(d.rgb, clamp(p[2].x / 255.0, 0.0, 1.0));
            textureStore(output, xy, clamp(result, vec4(0), vec4(1)));
        "},
    );

    // CopyColor
    composer.add_operation(
        4,
        indoc! {"
            let s = sample(source, coordinate(0u, at));
            let d = fetch(destination, vec2<i32>(floor(coordinate(1u, at))));
            let result = vec4(s.rgb, d.a);
            textureStore(output, xy, clamp(result, vec4(0), vec4(1)));
        "},
    );

    // CopyMask
    composer.add_operation(
        5,
        indoc! {"
            let s = sample(source, coordinate(0u, at));
            let d = fetch(destination, vec2<i32>(floor(coordinate(1u, at))));
            let result = vec4(d.rgb, s.a);
            textureStore(output, xy, clamp(result, vec4(0), vec4(1)));
        "},
    );

    // CopyOpaqueImage
    composer.add_operation(
        6,
        indoc! {"
            let s = sample(source, coordinate(0u, at));
            let result = vec4(s.rgb, 1.0);
            textureStore(output, xy, clamp(result, vec4(0), vec4(1)));
        "},
    );

    // AlphaBlend
    composer.add_operation(
        7,
        indoc! {"
            let s = sample(source, coordinate(0u, at));
            let d = fetch(destination, vec2<i32>(floor(coordinate(1u, at))));
            let opacity = clamp(p[2].x / 255.0, 0.0, 1.0);
            let result = over(d, s, opacity, u32(p[1].y));
            textureStore(output, xy, clamp(result, vec4(0), vec4(1)));
        "},
    );

    // ConstAlphaBlend
    composer.add_operation(
        8,
        indoc! {"
            let s = sample(source, coordinate(0u, at));
            let d = fetch(destination, vec2<i32>(floor(coordinate(1u, at))));
            let opacity = clamp(p[2].x / 255.0, 0.0, 1.0);
            let result = over(d, vec4(s.rgb, 1.0), opacity, u32(p[1].y));
            textureStore(output, xy, clamp(result, vec4(0), vec4(1)));
        "},
    );

    // ConstColorAlphaBlend
    composer.add_operation(
        9,
        indoc! {"
            let d = fetch(destination, vec2<i32>(floor(coordinate(1u, at))));
            let opacity = clamp(p[2].x / 255.0, 0.0, 1.0);
            let result = over(d, vec4(p[5].rgb, 1.0), opacity, u32(p[1].y));
            textureStore(output, xy, clamp(result, vec4(0), vec4(1)));
        "},
    );

    // ApplyColorMap
    composer.add_operation(
        10,
        indoc! {"
            let s = sample(source, coordinate(0u, at));
            let d = fetch(destination, vec2<i32>(floor(coordinate(1u, at))));
            let opacity = clamp(p[2].x / 255.0, 0.0, 1.0);
            let result = over(d, vec4(p[5].rgb, s.r), opacity, u32(p[1].y));
            textureStore(output, xy, clamp(result, vec4(0), vec4(1)));
        "},
    );

    // RemoveConstOpacity
    composer.add_operation(
        11,
        indoc! {"
            let d = fetch(destination, vec2<i32>(floor(coordinate(1u, at))));
            let opacity = clamp(p[2].x / 255.0, 0.0, 1.0);
            let result = vec4(d.rgb, d.a * (1.0 - opacity));
            textureStore(output, xy, clamp(result, vec4(0), vec4(1)));
        "},
    );

    // RemoveOpacity
    composer.add_operation(
        12,
        indoc! {"
            let s = sample(source, coordinate(0u, at));
            let d = fetch(destination, vec2<i32>(floor(coordinate(1u, at))));
            let result = vec4(d.rgb, d.a * (1.0 - s.r * abs(p[2].x) / 255.0));
            textureStore(output, xy, clamp(result, vec4(0), vec4(1)));
        "},
    );

    // ConstAlphaBlend_SD
    composer.add_operation(
        13,
        indoc! {"
            let s = sample(source, coordinate(0u, at));
            let d = fetch(destination, vec2<i32>(floor(coordinate(1u, at))));
            let opacity = clamp(p[2].x / 255.0, 0.0, 1.0);
            let result = mix(s, d, opacity);
            textureStore(output, xy, clamp(result, vec4(0), vec4(1)));
        "},
    );

    // UnivTransBlend
    composer.add_operation(
        14,
        indoc! {"
            let s = sample(source, coordinate(0u, at));
            let d = fetch(destination, vec2<i32>(floor(coordinate(1u, at))));
            let phase = p[2].z;
            let vague = max(p[2].w, 1.0);
            let mark = fetch(rule, vec2<i32>(floor(coordinate(2u, at)))).r * 255.0;
            let amount = clamp((phase - mark) / vague, 0.0, 1.0);
            let result = mix(s, d, amount);
            textureStore(output, xy, clamp(result, vec4(0), vec4(1)));
        "},
    );

    // AlphaToAdditiveAlpha
    composer.add_operation(
        15,
        indoc! {"
            let d = fetch(destination, vec2<i32>(floor(coordinate(1u, at))));
            let result = vec4(d.rgb * d.a, d.a);
            textureStore(output, xy, clamp(result, vec4(0), vec4(1)));
        "},
    );

    // AdditiveAlphaToAlpha
    composer.add_operation(
        16,
        indoc! {"
            let d = fetch(destination, vec2<i32>(floor(coordinate(1u, at))));
            let result = vec4(d.rgb / max(d.a, 0.000001), d.a);
            textureStore(output, xy, clamp(result, vec4(0), vec4(1)));
        "},
    );

    // DoGrayScale
    composer.add_operation(
        17,
        indoc! {"
            let d = fetch(destination, vec2<i32>(floor(coordinate(1u, at))));
            let result = vec4(vec3(dot(d.rgb, vec3(0.299, 0.587, 0.114))), d.a);
            textureStore(output, xy, clamp(result, vec4(0), vec4(1)));
        "},
    );

    // BoxBlur
    composer.add_operation(
        18,
        indoc! {"
            let kind = u32(p[1].y);
            var sum = vec4(0.0);
            var count = 0.0;
            for (var y = i32(p[3].y); y <= i32(p[3].w); y++) {
                for (var x = i32(p[3].x); x <= i32(p[3].z); x++) {
                    var v = fetch(previous, xy + vec2(x, y));
                    if (kind == 1u) {
                        v = vec4(v.rgb * v.a, v.a);
                    }
                    sum += v;
                    count += 1.0;
                }
            }
            var result = sum / max(count, 1.0);
            if (kind == 1u) {
                result = vec4(result.rgb / max(result.a, 0.000001), result.a);
            }
            textureStore(output, xy, clamp(result, vec4(0), vec4(1)));
        "},
    );

    // AdjustGamma
    composer.add_operation(
        19,
        indoc! {"
            let d = fetch(destination, vec2<i32>(floor(coordinate(1u, at))));
            let premultiplied = u32(p[1].y) == 2u;
            let target_color = p[5].xyz;
            let base_color = p[3].xyz;
            let exponent = max(p[4].xyz, vec3(0.000001));
            var rgb = d.rgb;
            if (premultiplied) {
                rgb /= max(d.a, 0.000001);
            }
            rgb = base_color + pow(clamp(rgb, vec3(0), vec3(1)), 1.0 / exponent) * (target_color - base_color);
            if (premultiplied) {
                rgb *= d.a;
            }
            let result = vec4(rgb, d.a);
            textureStore(output, xy, clamp(result, vec4(0), vec4(1)));
        "},
    );

    // Ps blends
    composer.add_operation(
        20,
        indoc! {"
            let s = sample(source, coordinate(0u, at));
            let d = fetch(destination, vec2<i32>(floor(coordinate(1u, at))));
            let opacity = clamp(p[2].x / 255.0, 0.0, 1.0);
            let result = vec4(mix(d.rgb, blend_color(d.rgb, s.rgb, u32(p[2].y)), s.a * opacity), d.a);
            textureStore(output, xy, clamp(result, vec4(0), vec4(1)));
        "},
    );

    // non-photoshop blend
    composer.add_operation(
        21,
        indoc! {"
            let s = sample(source, coordinate(0u, at));
            let d = fetch(destination, vec2<i32>(floor(coordinate(1u, at))));
            let opacity = clamp(p[2].x / 255.0, 0.0, 1.0);
            var result = vec4(d.rgb, d.a);
            if (u32(p[2].y) == 8u) {
                result = vec4(kirikiri_dodge(d.rgb, s.rgb, u32(clamp(p[2].x, 0.0, 255.0))), d.a);
            } else {
                result = vec4(mix(d.rgb, blend_color(d.rgb, s.rgb, u32(p[2].y)), opacity), d.a);
            }
            textureStore(output, xy, clamp(result, vec4(0), vec4(1)));
        "},
    );

    #[cfg(feature = "video-ffmpeg")]
    {
        // VideoColor
        composer.add_operation(
            22,
            indoc! {"
                let s = sample(source, coordinate(0u, at));
                if (all(p[3] == vec4(0.0, 1.0, 0.0, 1.0))) {
                    textureStore(output, xy, clamp(s, vec4(0), vec4(1)));
                } else {
                    let y = dot(s.rgb, vec3(0.299, 0.587, 0.114));
                    let cb = (s.b - y) / 1.772;
                    let cr = (s.r - y) / 1.402;
                    let angle = radians(p[3].z);
                    let chroma = vec2(cb * cos(angle) - cr * sin(angle), cb * sin(angle) + cr * cos(angle)) * p[3].w;
                    let luma = y * p[3].y + p[3].x;
                    let result = vec4(luma + 1.402 * chroma.y, luma - 0.344136 * chroma.x - 0.714136 * chroma.y, luma + 1.772 * chroma.x, s.a);
                    textureStore(output, xy, clamp(result, vec4(0), vec4(1)));
                }
            "},
        );
    }

    // LayerMovieAlpha
    composer.add_operation(
        30,
        indoc! {"
            let s = sample(source, coordinate(0u, at));
            let mask = fetch(source, vec2<i32>(floor(coordinate(0u, at))) + vec2<i32>(i32(p[3].x), 0));
            let result = vec4(s.rgb, mask.r);
            textureStore(output, xy, clamp(result, vec4(0), vec4(1)));
        "},
    );

    // horizontal blur
    composer.add_operation(
        40,
        indoc! {"
            let radius = i32(p[3].x);
            let lo = max(xy - vec2(radius, 0), vec2(0));
            let hi = min(xy + vec2(radius, 0), vec2<i32>(textureDimensions(source)) - 1);
            var sum = vec4(0.0);
            for (var x = lo.x; x <= hi.x; x++) {
                sum += round(fetch(source, vec2(x, xy.y)) * 255.0);
            }
            let result = floor(sum / f32(hi.x - lo.x + 1)) / 255.0;
            textureStore(output, xy, clamp(result, vec4(0), vec4(1)));
        "},
    );

    // vertical blur
    composer.add_operation(
        41,
        indoc! {"
            let radius = i32(p[3].y);
            let lo = max(xy - vec2(0, radius), vec2(0));
            let hi = min(xy + vec2(0, radius), vec2<i32>(textureDimensions(source)) - 1);
            var sum = vec4(0.0);
            for (var y = lo.y; y <= hi.y; y++) {
                sum += round(fetch(source, vec2(xy.x, y)) * 255.0);
            }
            let result = floor(sum / f32(hi.y - lo.y + 1)) / 255.0;
            textureStore(output, xy, clamp(result, vec4(0), vec4(1)));
        "},
    );
}

fn layerex_operations(composer: &mut ShaderComposer) {
    composer.funcs.push(Function {
        name: "hue_rgb",
        params: vec![("n1", "f32"), ("n2", "f32"), ("input", "f32")],
        ret: "f32",
        body: indoc! {"
            var h = input;
            if (h > 360.0) {
                h -= 360.0;
            } else if (h < 0.0) {
                h += 360.0;
            }
            if (h < 60.0) {
                return n1 + (n2 - n1) * h / 60.0;
            }
            if (h < 180.0) {
                return n2;
            }
            if (h < 240.0) {
                return n1 + (n2 - n1) * (240.0 - h) / 60.0;
            }
            return n1;
        "},
    });
    composer.funcs.push(Function {
        name: "hsl_rgb",
        params: vec![("h", "f32"), ("s", "f32"), ("l", "f32")],
        ret: "vec3<f32>",
        body: indoc! {"
            let n2 = select(l + s - l * s, l * (1.0 + s), l <= 0.5);
            let n1 = 2.0 * l - n2;
            if (s == 0.0) {
                return vec3(l);
            }
            return vec3(hue_rgb(n1, n2, h + 120.0), hue_rgb(n1, n2, h), hue_rgb(n1, n2, h - 120.0));
        "},
    });
    composer.funcs.push(Function {
        name: "hash_noise",
        params: vec![("input", "u32")],
        ret: "f32",
        body: indoc! {"
            var x = input;
            x ^= x >> 16u;
            x *= 0x7feb352du;
            x ^= x >> 15u;
            x *= 0x846ca68bu;
            x ^= x >> 16u;
            return f32(x) / 4294967295.0;
        "},
    });

    // LayerExLight
    composer.add_operation(
        23,
        indoc! {"
            let d = fetch(destination, vec2<i32>(floor(coordinate(1u, at))));
            let rgb = floor(d.rgb * 255.0 + 0.5);
            let contrast = (100.0 + trunc(p[3].y)) / 100.0;
            let result = vec4(clamp(trunc((rgb - 128.0) * contrast + trunc(p[3].x) + 128.0), vec3(0), vec3(255)) / 255.0, d.a);
            textureStore(output, xy, clamp(result, vec4(0), vec4(1)));
        "},
    );

    // LayerExColorize
    composer.add_operation(
        24,
        indoc! {"
            let d = fetch(destination, vec2<i32>(floor(coordinate(1u, at))));
            let rgb = vec3<i32>(floor(d.rgb * 255.0 + 0.5));
            let hi = max(max(rgb.r, rgb.g), rgb.b);
            let lo = min(min(rgb.r, rgb.g), rgb.b);
            let lightness = ((hi + lo) * 255 + 255) / 510;
            let hue = f32(i32(p[3].x) & 255) * 360.0 / 255.0;
            let saturation = f32(i32(p[3].y) & 255) / 255.0;
            let colored = trunc(hsl_rgb(hue, saturation, f32(lightness) / 255.0) * 255.0);
            let amount = p[3].z;
            var out = floor((colored * amount + vec3<f32>(rgb) * (256.0 - amount)) / 256.0);
            if (p[3].w != 0.0) {
                out = colored;
            }
            let result = vec4(out / 255.0, d.a);
            textureStore(output, xy, clamp(result, vec4(0), vec4(1)));
        "},
    );

    // LayerExModulate
    composer.add_operation(
        25,
        indoc! {"
            let d = fetch(destination, vec2<i32>(floor(coordinate(1u, at))));
            let hi = max(max(d.r, d.g), d.b);
            let lo = min(min(d.r, d.g), d.b);
            let delta = hi - lo;
            let add = hi + lo;
            var lightness = add / 2.0;
            var saturation = 0.0;
            var hue = 0.0;
            if (delta != 0.0) {
                saturation = delta / select(2.0 - add, add, lightness < 0.5);
                if (d.r == hi) {
                    hue = (d.g - d.b) / delta;
                } else if (d.g == hi) {
                    hue = 2.0 + (d.b - d.r) / delta;
                } else {
                    hue = 4.0 + (d.r - d.g) / delta;
                }
                hue /= 6.0;
            }
            hue = fract(hue + trunc(p[3].x) / 360.0);
            let ds = trunc(p[3].y) / 100.0;
            let dl = trunc(p[3].z) / 100.0;
            saturation += select(saturation, 1.0 - saturation, ds > 0.0) * ds;
            lightness += select(lightness, 1.0 - lightness, dl > 0.0) * dl;
            let result = vec4(trunc(hsl_rgb(hue * 360.0, saturation, lightness) * 255.0) / 255.0, d.a);
            textureStore(output, xy, clamp(result, vec4(0), vec4(1)));
        "},
    );

    let noise = indoc! {"
        let d = fetch(destination, vec2<i32>(floor(coordinate(1u, at))));
        let seed = u32(xy.x) + u32(xy.y) * textureDimensions(output).x + u32(p[3].w) * 747796405u;
        let n = vec3<f32>(hash_noise(seed), hash_noise(seed + 0x1234567u), hash_noise(seed + 0x7654321u));
        var result = vec4(vec3(floor(n.x * 255.0) / 255.0), d.a);
        if (operation == 26u) {
            let amount = trunc((n - 0.5) * trunc(p[3].x));
            result = vec4(clamp(floor(d.rgb * 255.0 + 0.5) + amount, vec3(0), vec3(255)) / 255.0, d.a);
        }
        textureStore(output, xy, clamp(result, vec4(0), vec4(1)));
    "};

    // LayerExNoise
    composer.add_operation(26, noise);

    // LayerExWhiteNoise
    composer.add_operation(27, noise);

    let gaussian = indoc! {"
        let axis = select(0, 1, operation == 29u);
        let length = i32(p[3].y);
        let mid = length / 2;
        let low = i32(p[0][axis]);
        let high = i32(p[0][axis + 2]);
        let row = xy[axis] - low;
        let size = high - low;
        var sum = vec4(0.0);
        var scale = 0.0;
        for (var j = 0; j < length; j++) {
            let position = row + j - mid;
            if (position >= 0 && position < size) {
                var sample_at = xy;
                sample_at[axis] = position + low;
                let w = p[26u + u32(j)].x;
                scale += w;
                var numerator = w;
                if (length > size) {
                    numerator = p[26u + u32(position)].x;
                }
                sum += floor(fetch(previous, sample_at) * 255.0 + 0.5) * numerator;
            }
        }
        let result = vec4<f32>(vec4<u32>(floor(sum / scale + 0.5)) & vec4<u32>(255u)) / 255.0;
        textureStore(output, xy, clamp(result, vec4(0), vec4(1)));
    "};

    // LayerExGaussianH
    composer.add_operation(28, gaussian);

    // LayerExGaussianV
    composer.add_operation(29, gaussian);

    // LayerExRaster
    composer.add_operation(
        31,
        indoc! {"
            let shift = i32(p[26u + u32(xy.y)].x);
            let origin = xy - vec2(shift, 0);
            var result = fetch(previous, xy);
            if (f32(origin.x) >= p[0].x && f32(origin.x) < p[0].z) {
                result = fetch(source, origin);
            }
            textureStore(output, xy, clamp(result, vec4(0), vec4(1)));
        "},
    );

    // LayerExTextPaint
    composer.add_operation(
        35,
        indoc! {"
            let old = fetch(previous, xy);
            let origin = xy - vec2<i32>(p[3].yz);
            var coverage = 0.0;
            let radius = i32(ceil(p[3].x));
            for (var y = -radius; y <= radius; y++) {
                for (var x = -radius; x <= radius; x++) {
                    if (f32(x * x + y * y) <= p[3].x * p[3].x + 0.5) {
                        let q = origin + vec2(x, y);
                        if (all(q >= vec2(0)) && all(q < vec2<i32>(textureDimensions(source)))) {
                            coverage = max(coverage, fetch(source, q).a);
                        }
                    }
                }
            }
            let direction = p[4].zw - p[4].xy;
            var amount = 0.0;
            if (p[3].w != 0.0) {
                amount = dot(vec2<f32>(origin) - p[4].xy, direction) / max(dot(direction, direction), 0.000001);
                let wrap = u32(p[3].z);
                if (wrap == 0u) {
                    amount = fract(amount);
                } else if (wrap == 1u) {
                    amount = 1.0 - abs((fract(amount * 0.5) * 2.0) - 1.0);
                } else if (wrap == 2u) {
                    amount = fract(amount);
                } else if (wrap == 3u) {
                    amount = 1.0 - abs((fract(amount * 0.5) * 2.0) - 1.0);
                } else {
                    amount = clamp(amount, 0.0, 1.0);
                }
            }
            let ink = mix(p[2], p[5], amount);
            let result = over(old, vec4(ink.rgb, ink.a * coverage), 1.0, 1u);
            textureStore(output, xy, clamp(result, vec4(0), vec4(1)));
        "},
    );
}

fn extrans_operations(composer: &mut ShaderComposer) {
    composer.funcs.push(Function {
        name: "transition_blend",
        params: vec![
            ("a", "vec4<f32>"),
            ("b", "vec4<f32>"),
            ("ratio", "f32"),
            ("layer", "u32"),
        ],
        ret: "vec4<f32>",
        body: indoc! {"
            let f = ratio / 255.0;
            if (layer == 1u) {
                return vec4(mix(a.rgb, b.rgb, f), 1.0);
            }
            let alpha = mix(a.a, b.a, f);
            if (layer == 2u) {
                return vec4(mix(a.rgb * a.a, b.rgb * b.a, f) / max(alpha, 0.000001), alpha);
            }
            return mix(a, b, f);
        "},
    });
    composer.funcs.push(Function {
        name: "transition_blur",
        params: vec![
            ("t", "texture_2d<f32>"),
            ("xy", "vec2<i32>"),
            ("radius", "vec2<i32>"),
            ("bilinear", "bool"),
        ],
        ret: "vec4<f32>",
        body: indoc! {"
            if (bilinear) {
                let stride = radius * 2 + 1;
                let lo = xy / stride * stride;
                let hi = min(lo + stride, vec2<i32>(textureDimensions(t)) - 1);
                let f = vec2<i32>((xy - lo) * 255 / max(hi - lo, vec2(1)));
                let top = nb(fetch(t, lo), fetch(t, vec2(hi.x, lo.y)), f32(f.x));
                let bottom = nb(fetch(t, vec2(lo.x, hi.y)), fetch(t, hi), f32(f.x));
                return nb(top, bottom, f32(f.y));
            }
            var sum = vec4(0.0);
            let lo = max(xy - radius, vec2(0));
            let hi = min(xy + radius, vec2<i32>(textureDimensions(t)) - 1);
            for (var y = lo.y; y <= hi.y; y++) {
                var row = vec4(0.0);
                for (var x = lo.x; x <= hi.x; x++) {
                    row += round(fetch(t, vec2(x, y)) * 255.0);
                }
                sum += floor(row / f32(hi.x - lo.x + 1));
            }
            return floor(sum / f32(hi.y - lo.y + 1)) / 255.0;
        "},
    });

    // ExtransWave
    composer.add_operation(
        32,
        indoc! {"
            let local = vec2<i32>(floor(coordinate(0u, at))) - vec2<i32>(p[6].xy);
            let x = local.x - i32(p[26u + u32(local.y)].x);
            var result = p[5];
            if (x >= 0 && x < i32(p[6].z - p[6].x)) {
                let a = ns(vec2(x, local.y), 0);
                let b = ns(vec2(x, local.y), 1);
                if (u32(p[1].z) == 2u) {
                    result = transition_blend(a, b, p[2].x, 2u);
                } else {
                    result = nb(a, b, p[2].x);
                }
            }
            textureStore(output, xy, clamp(result, vec4(0), vec4(1)));
        "},
    );

    // ExtransMosaic
    composer.add_operation(
        33,
        indoc! {"
            let size = max(p[3].x, 2.0);
            let offset = p[3].yz;
            let q = floor(coordinate(0u, at));
            let origin = vec2<i32>(floor((q - offset) / size) * size + offset + floor(size / 2.0));
            let q2 = origin + vec2<i32>(p[7].xy - p[6].xy);
            let opacity = clamp(p[2].x / 255.0, 0.0, 1.0);
            let result = mix(fetch(source, origin), fetch(destination, q2), opacity);
            textureStore(output, xy, clamp(result, vec4(0), vec4(1)));
        "},
    );

    // ExtransBlurFade
    composer.add_operation(
        34,
        indoc! {"
            let source_at = vec2<i32>(floor(coordinate(0u, at)));
            let destination_at = vec2<i32>(floor(coordinate(1u, at)));
            var a: vec4<f32>;
            var b: vec4<f32>;
            if (p[4].y == 1.0) {
                a = fetch(source, source_at);
                b = fetch(destination, destination_at);
            } else {
                a = transition_blur(source, source_at, vec2<i32>(p[3].xy), true);
                b = transition_blur(destination, destination_at, vec2<i32>(p[3].zw), true);
            }
            let result = nb(a, b, p[2].x);
            textureStore(output, xy, clamp(result, vec4(0), vec4(1)));
        "},
    );

    // ExtransTurn
    composer.add_operation(
        36,
        indoc! {"
            let logical = xy - vec2<i32>(p[0].xy) + vec2<i32>(p[2].zw);
            let tile = logical / 64;
            let phase = clamp(i32(p[3].x) - (tile.x - tile.y) * 2, 0, 63);
            let local = logical - tile * 64;
            var q = logical;
            var covered = true;
            if (phase != 0 && phase != 63) {
                let j = 26u + u32((phase * 64 + local.y) * 2);
                let row = bitcast<vec4<i32>>(p[j]);
                let step = bitcast<vec4<i32>>(p[j + 1u]);
                covered = local.x >= row.x && local.x < row.x + row.y;
                q = tile * 64 + ((row.zw + (local.x - row.x) * step.xy) >> vec2<u32>(16));
                covered = covered && all(q >= vec2(0)) && all(q < vec2<i32>(p[3].yz));
            }
            var result = p[5];
            if (covered) {
                if (phase < 32) {
                    result = fetch(source, q + vec2<i32>(p[6].xy - p[2].zw));
                } else {
                    result = fetch(destination, q + vec2<i32>(p[7].xy - p[2].zw));
                }
                let gloss = array<i32, 13>(0, 0, 0, 0, 16, 48, 80, 128, 192, 128, 80, 48, 16);
                if (phase < 13) {
                    let amount = f32(gloss[u32(phase)]);
                    let bytes = round(result * 255.0);
                    result = floor(bytes + (vec4(255.0, 255.0, 255.0, 0.0) - bytes) * amount / 256.0) / 255.0;
                }
            }
            textureStore(output, xy, clamp(result, vec4(0), vec4(1)));
        "},
    );

    // ExtransRotate
    composer.add_operation(
        37,
        indoc! {"
            let logical = xy - vec2<i32>(p[0].xy) + vec2<i32>(p[2].zw);
            var result = p[5];
            if (logical.y >= 0 && logical.y < i32(p[3].z)) {
                let j = 26u + u32(logical.y) * 10u;
                let count = bitcast<vec4<i32>>(p[j]).x;
                for (var i = 0; i < count; i++) {
                    let region = bitcast<vec4<i32>>(p[j + 5u + u32(i)]);
                    if (logical.x < region.x || logical.x >= region.y || region.z == 0) {
                        continue;
                    }
                    let k = j + 1u + u32(region.z - 1) * 2u;
                    let row = bitcast<vec4<i32>>(p[k]);
                    let stepy = bitcast<vec4<i32>>(p[k + 1u]).x;
                    let q = (row.yz + (logical.x - row.x) * vec2(row.w, stepy)) >> vec2<u32>(16);
                    if (all(q >= vec2(0)) && all(q < vec2<i32>(p[3].yz))) {
                        if (region.z == 1) {
                            result = fetch(source, q + vec2<i32>(p[6].xy - p[2].zw));
                        } else {
                            result = fetch(destination, q + vec2<i32>(p[7].xy - p[2].zw));
                        }
                    }
                }
            }
            textureStore(output, xy, clamp(result, vec4(0), vec4(1)));
        "},
    );

    // ExtransRipple
    composer.add_operation(
        38,
        indoc! {"
            let opacity = clamp(p[2].x / 255.0, 0.0, 1.0);
            let center = p[3].yz;
            let delta = at - center;
            let distance = length(vec2(delta.x, delta.y / max(p[4].x, 0.0001)));
            let envelope = sin(clamp(distance / max(f32(textureDimensions(source).x), 1.0), 0.0, 1.0) * 3.14159265);
            let phase = distance / max(p[3].w, 1.0) * 6.2831853 - p[4].y * opacity * 6.2831853;
            let drift = sin(phase) * envelope * p[4].z * sin(3.14159265 * opacity);
            let direction = normalize(delta + vec2(0.0001));
            let mapped = vec2<i32>(at + direction * drift);
            let result = mix(fetch(source, mapped), fetch(destination, mapped), opacity);
            textureStore(output, xy, clamp(result, vec4(0), vec4(1)));
        "},
    );
}

fn nagano_operations(composer: &mut ShaderComposer) {
    // ExtNagano
    composer.add_operation(
        39,
        indoc! {"
            let q = xy - vec2<i32>(p[0].xy) + vec2<i32>(p[2].zw);
            let result = nagano(q);
            textureStore(output, xy, clamp(result, vec4(0), vec4(1)));
        "},
    );
}

#[cfg(test)]
mod verify {
    use super::*;

    #[test]
    fn compose_compiles() {
        let mut resolver = wesl::resolver::VirtualResolver::new();
        resolver.add_module("package::krkr::operations".parse().unwrap(), compose().into());
        resolver.add_module(
            "package::krkr::extNagano".parse().unwrap(),
            include_str!("nagano.wesl").into(),
        );
        let mut compiler = wesl::Compiler::default().with_resolver(resolver);
        compiler.options.keep_main = true;
        let main: wesl::syntax::ModulePath = "package::krkr::operations".parse().unwrap();
        if let Err(error) = compiler.compile_module(&main) {
            panic!("\n{error}");
        }
    }
}
