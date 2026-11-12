//! assemble operation shader

use bevy::platform::collections::HashMap;
use indoc::indoc;
use itertools::Itertools;

fn s(s: &str) -> String {
    String::from(s)
}

struct Function {
    name: String,
    params: Vec<(String, String)>,
    ret: String,
    body: String,
}

impl Function {
    fn compose(&self) -> String {
        format!(
            "fn {}({})->{}{{{}}}",
            self.name,
            self.params
                .iter()
                .map(|(name, ty)| format!("name: ty"))
                .join(","),
            self.ret,
            self.body,
        )
    }
}

struct ShaderComposer {
    bindings: HashMap<u32, (String, Option<String>, String)>,
    consts: Vec<(String, String, String)>,
    funcs: Vec<Function>,
    operations: HashMap<u32, String>,
}

fn builtin_funcs() -> Vec<Function> {
    vec![
        Function {
            name: s("edge"),
            params: vec![
                (s("a"), s("vec2<f32>")),
                (s("b"), s("vec2<f32>")),
                (s("v"), s("vec2<f32>")),
            ],
            ret: s("f32"),
            body: s("return (b.x - a.x) * (v.y - a.y) - (b.y - a.y) * (v.x - a.x);")
        },
        Function {
        name: s("inside"),
        params: vec![(s("at"), s("vec2<f32>"))],
        ret: s("bool"),
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
            "}.to_string(),
        },
        Function {
            name: s("coordinate"),
            params: vec![
                (s("i"), s("u32")),
                (s("at"), s("vec2<f32>")),
            ],
            ret: s("vec2<f32>"),
            body: indoc! {"
                if (p[1].w == 0.0) {
                    return mix(p[6 + i].xy p[6 + i].zw, (at - p[0].xy) / (p[0].zw - p[0].xy));
                }
                let j = 17 + i * 3;
                let q = vec3(dot(p[j].xyz, vec3(at, 1)), dot(p[j + 1].xyz, vec3(at, 1)), dot(p[j + 2].xyz, vec3(at, 1)));
                return q.xy / q.z;
            "}.to_string()
        },
        Function {
            name: s("fetch"),
            params: vec![
                (s("t"), s("texture_2d<f32>")),
                (s("at"), s("vec2<i32>"))
            ],
            ret: s("vec4<f32>"),
            body: s("return textureLoad(t, clamp(at, vec2(0), vec2<i32>(textureDimensions(t)) - 1), 0);"),
        },
        Function {
            name: s("source_fetch"),
            params: vec![
                (s("t"), s("texture_2d<f32>")),
                (s("at"), s("vec2<i32>"))
            ],
            ret: s("vec4<f32>"),
            body: indoc! {"
                if (p[1].w != 0.0) {
                    return fetch(t,at);
                }
                let low = vec2<i32>(floor(min(p[6].xy, p[6].zw)));
                let high = vec2<i32>(ceil(max(p[6].xy, p[6].zw))) - 1;
                return fetch(t, clamp(at, low, high));
            "}.to_string(),
        },
        Function {
            name: s("sinc"),
            params: vec![(s("x"), s("f32"))],
            ret: s("f32"),
            body: indoc! {"
                if (abs(x) < 0.000001) {
                    return 1.0;
                }
                return sin(3.14159265359 * x) / (3.14159265359 * x);
            "}.to_string(),
        }
        Function {
            name: s("weight"),
            params: vec![
                (s("distance"), s("f32")),
                (s("mode"), s("u32")),
            ],
            ret: s("f32"),
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
                        if (x<2.0) {
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
            "}.to_string(),
        },
        Function {
            name: s("sample"),
            params: vec![
                (s("t"), s("texture_2d<f32>")),
                (s("at"), s("vec2<f32>"))
            ],
            ret: s("vec4<f32>"),
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
            "}.to_string(),
        }
    ]
}

impl ShaderComposer {
    pub fn new() -> Self {
        let mut output = Self {
            bindings: HashMap::new(),
            consts: Vec::new(),
            funcs: Vec::new(),
            operations: HashMap::new(),
        };

        // @group(0) @binding(0) var source: texture_2d<f32>;
        output.bindings.insert(0, (s("source"), None, s("texture_2d<f32>")));
        // @group(0) @binding(1) var destination: texture_2d<f32>;
        output.bindings.insert(1, (s("destination"), None, s("texture_2d<f32>")));
        // @group(0) @binding(2) var rule: texture_2d<f32>;
        output.bindings.insert(2, (s("rule"), None, s("texture_2d<f32>")));
        // @group(0) @binding(3) var previous: texture_2d<f32>;
        output.bindings.insert(3, (s("previous"), None, s("texture_2d<f32>")));
        // @group(0) @binding(4) var output: texture_storage_2d<rgba8unorm, write>;
        output.bindings.insert(4, (s("output"), None, s("texture_storage_2d<rgba8unorm, write>")));
        // @group(0) @binding(5) var<storage, read> p: array<vec4<f32>>;
        output.bindings.insert(5, (s("s"), Some(s("storage, read")), s("array<vec4<f32>>")));

        output.funcs.extend(builtin_funcs());

        output
    }

    pub fn compose(&self) -> String {
        let mut output = String::new();

        self.consts.iter().for_each(|(name, ty, value)| {
            output.push_str(&format!("const {name}:{ty}={value};\n"));
        });
        self.bindings.iter().for_each(|(binding, param)| {
            output.push_str(&format!(
                "@group(0) @binding({binding}) var{} {}:{};\n",
                param.0,
                if let Some(ref ty) = param.1 {
                    format!("<ty>")
                } else {
                    String::new()
                },
                param.2,
            ));
        });
        self.funcs.iter().for_each(|func| {
            output.push_str(&func.compose());
            output.push('\n');
        });

        output.push_str(indoc! {"
        @compute @workgroup_size(8,8)
        fn main(@builtin(global_invocation_id) invocation: vec3<u32>) {
            let xy = vec2<i32>(invocation.xy) + vec2<i32>(max(p[0].xy, vec2(0)));
            let at = vec2<f32>(xy) + 0.5; // pixel center
            if any(xy >= vec2<i32>(textureDimensions(output))) || any(at >= p[0].zw) || !inside(at) {
                return;
            }

            let operation = u32(p[1].x);
        "});

        output.push('}');
        output
    }
}
