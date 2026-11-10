@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var destination: texture_2d<f32>;
@group(0) @binding(2) var rule: texture_2d<f32>;
@group(0) @binding(3) var previous: texture_2d<f32>;
@group(0) @binding(4) var output: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(5) var<storage, read> p: array<vec4<f32>>;

fn fetch(t: texture_2d<f32>, at: vec2<i32>) -> vec4<f32> {
    return textureLoad(t, clamp(at, vec2(0), vec2<i32>(textureDimensions(t))-1), 0);
}
fn source_fetch(t: texture_2d<f32>, at: vec2<i32>) -> vec4<f32> {
    if (p[1].w != 0.0) { return fetch(t,at); }
    let low = vec2<i32>(floor(min(p[6].xy,p[6].zw)));
    let high = vec2<i32>(ceil(max(p[6].xy,p[6].zw)))-1;
    return fetch(t,clamp(at,low,high));
}
fn sinc(x: f32) -> f32 {
    if (abs(x) < 0.000001) { return 1.0; }
    return sin(3.14159265359*x)/(3.14159265359*x);
}
fn weight(distance: f32, mode: u32) -> f32 {
    let x = abs(distance);
    switch mode {
        case 3u,5u: {
            if (x<=1.0) { return 1.0-2.0*x*x+x*x*x; }
            if (x<2.0) { return 4.0-8.0*x+5.0*x*x-x*x*x; }
        }
        case 6u,7u: { if (x<2.0) { return sinc(x)*sinc(x/2.0); } }
        case 8u,9u: { if (x<3.0) { return sinc(x)*sinc(x/3.0); } }
        case 10u,11u: {
            if (x<=1.0) { return x*x*x-1.8*x*x-0.2*x+1.0; }
            if (x<2.0) { return -x*x*x/3.0+1.8*x*x-46.0*x/15.0+1.6; }
        }
        case 12u,13u: {
            if (x<=1.0) { return 13.0*x*x*x/11.0-453.0*x*x/209.0-3.0*x/209.0+1.0; }
            if (x<=2.0) { return -6.0*x*x*x/11.0+612.0*x*x/209.0-1038.0*x/209.0+540.0/209.0; }
            if (x<3.0) { return x*x*x/11.0-159.0*x*x/209.0+434.0*x/209.0-384.0/209.0; }
        }
        case 16u,17u: { if (x<2.0) { return exp(-2.0*x*x); } }
        case 18u,19u: {
            if (x<4.0) { return sinc(x)*(0.42+0.5*cos(3.14159265359*x/4.0)+0.08*cos(3.14159265359*x/2.0)); }
        }
        default: { return max(1.0-x,0.0); }
    }
    return 0.0;
}
fn sample(t: texture_2d<f32>, at: vec2<f32>) -> vec4<f32> {
    let mode = u32(p[1].z);
    if (mode==0u) { return source_fetch(t,vec2<i32>(floor(at))); }
    let q = at-0.5;
    let a = vec2<i32>(floor(q));
    let f = fract(q);
    if (mode==1u || mode==2u || mode==4u) {
        return mix(mix(source_fetch(t,a),source_fetch(t,a+vec2(1,0)),f.x),mix(source_fetch(t,a+vec2(0,1)),source_fetch(t,a+vec2(1,1)),f.x),f.y);
    }
    var scale = vec2(1.0);
    if (p[1].w==0.0) { scale = max(abs((p[6].zw-p[6].xy)/(p[0].zw-p[0].xy)),vec2(1)); }
    var radius = 2.0;
    if (mode==8u || mode==9u || mode==12u || mode==13u) { radius = 3.0; }
    if (mode==18u || mode==19u) { radius = 4.0; }
    let area = mode==14u || mode==15u;
    if (area) { radius = 0.5; }
    var sum = vec4(0.0); var total = 0.0;
    let first = vec2<i32>(floor(q-radius*scale));
    let last = vec2<i32>(ceil(q+radius*scale));
    for (var y=first.y;y<=last.y;y++) {
        for (var x=first.x;x<=last.x;x++) {
            let distance = vec2<f32>(f32(x),f32(y))-q;
            var contribution = weight(distance.x/scale.x,mode)*weight(distance.y/scale.y,mode);
            if (area) {
                let overlap = max(min(vec2<f32>(f32(x),f32(y))+0.5,q+scale*0.5)-max(vec2<f32>(f32(x),f32(y))-0.5,q-scale*0.5),vec2(0));
                contribution = overlap.x*overlap.y;
            }
            sum += source_fetch(t,vec2(x,y))*contribution; total += contribution;
        }
    }
    return sum/max(total,0.000001);
}
fn coordinate(i: u32, at: vec2<f32>) -> vec2<f32> {
    if (p[1].w == 0.0) {
        return mix(p[6+i].xy,p[6+i].zw,(at-p[0].xy)/(p[0].zw-p[0].xy));
    }
    let j = 17 + i*3;
    let q = vec3(dot(p[j].xyz,vec3(at,1)),dot(p[j+1].xyz,vec3(at,1)),dot(p[j+2].xyz,vec3(at,1)));
    return q.xy/q.z;
}
fn blend_color(d: vec3<f32>, s: vec3<f32>, mode: u32) -> vec3<f32> {
    switch mode {
        case 1u: { return min(d+s,vec3(1)); }
        case 2u: { return max(d+s-1.0,vec3(0)); }
        case 3u: { return d*s; }
        case 4u: { return 1.0-(1.0-d)*(1.0-s); }
        case 5u: { return select(2.0*d*s,1.0-2.0*(1.0-d)*(1.0-s),d>vec3(0.5)); }
        case 6u: { return select(2.0*d*s,1.0-2.0*(1.0-d)*(1.0-s),s>vec3(0.5)); }
        case 7u: { return select(d-(1.0-2.0*s)*d*(1.0-d),d+(2.0*s-1.0)*(sqrt(d)-d),s>vec3(0.5)); }
        case 8u: { return min(d/max(1.0-s,vec3(0.000001)),vec3(1)); }
        case 9u: { return 1.0-min((1.0-d)/max(s,vec3(0.000001)),vec3(1)); }
        case 10u: { return max(d,s); }
        case 11u: { return min(d,s); }
        case 12u: { return abs(d-s); }
        case 13u: { return d+s-2.0*d*s; }
        default: { return s; }
    }
}
fn kirikiri_dodge(d: vec3<f32>, s: vec3<f32>, opacity: u32) -> vec3<f32> {
    if (opacity == 0u) { return d; }
    var source_bytes = vec3<u32>(round(clamp(s,vec3(0),vec3(1))*255.0));
    if (opacity < 255u) { source_bytes = (source_bytes*opacity) >> vec3(8u); }
    let destination_bytes = vec3<u32>(round(clamp(d,vec3(0),vec3(1))*255.0));
    let reciprocal = vec3(65536u)/max(vec3(255u)-source_bytes,vec3(1u));
    return vec3<f32>(min((destination_bytes*reciprocal) >> vec3(8u),vec3(255u)))/255.0;
}
fn over(d: vec4<f32>, s: vec4<f32>, opacity: f32, kind: u32) -> vec4<f32> {
    let alpha = s.a*opacity;
    let out_alpha = alpha+d.a*(1.0-alpha);
    if (kind == 1u) {
        return vec4((s.rgb*alpha+d.rgb*d.a*(1.0-alpha))/max(out_alpha,0.000001),out_alpha);
    }
    if (kind == 2u) { return vec4(s.rgb*alpha+d.rgb*(1.0-alpha),out_alpha); }
    if (kind == 3u) { return vec4(s.rgb*opacity+d.rgb*(1.0-alpha),d.a); }
    if (kind == 4u) { return vec4(s.rgb*opacity+d.rgb*(1.0-alpha),out_alpha); }
    if (kind == 5u) { return vec4((s.rgb*opacity+d.rgb*d.a*(1.0-alpha))/max(out_alpha,0.000001),out_alpha); }
    return vec4(mix(d.rgb,s.rgb,alpha),d.a);
}
fn edge(a: vec2<f32>, b: vec2<f32>, v: vec2<f32>) -> f32 {
    return (b.x-a.x)*(v.y-a.y)-(b.y-a.y)*(v.x-a.x);
}
fn inside(at: vec2<f32>) -> bool {
    if (p[1].w == 0.0) { return true; }
    let a = p[9].xy; let b = p[9].zw; let c = p[10].xy;
    let ab = edge(a,b,at); let bc = edge(b,c,at); let ca = edge(c,a,at);
    if (p[1].w == 3.0) {
        let winding = sign(edge(a,b,c));
        let edges = vec3(ab,bc,ca)*winding;
        let delta = array<vec2<f32>,3>((b-a)*winding,(c-b)*winding,(a-c)*winding);
        for (var i=0u;i<3u;i++) {
            if (edges[i]<0.0) { return false; }
            if (edges[i]==0.0 && !(delta[i].y<0.0 || (delta[i].y==0.0 && delta[i].x>0.0))) { return false; }
        }
        return true;
    }
    let d = p[10].zw;
    let e = vec4(edge(a,b,at),edge(b,d,at),edge(d,c,at),edge(c,a,at));
    return all(e>=vec4(0)) || all(e<=vec4(0));
}
fn hue_rgb(n1: f32, n2: f32, input: f32) -> f32 {
    var h = input;
    if h > 360.0 { h -= 360.0; } else if h < 0.0 { h += 360.0; }
    if h < 60.0 { return n1+(n2-n1)*h/60.0; }
    if h < 180.0 { return n2; }
    if h < 240.0 { return n1+(n2-n1)*(240.0-h)/60.0; }
    return n1;
}
fn hsl_rgb(h: f32, s: f32, l: f32) -> vec3<f32> {
    let n2 = select(l+s-l*s,l*(1.0+s),l<=0.5);
    let n1 = 2.0*l-n2;
    if s == 0.0 { return vec3(l); }
    return vec3(hue_rgb(n1,n2,h+120.0),hue_rgb(n1,n2,h),hue_rgb(n1,n2,h-120.0));
}
fn hash_noise(input: u32) -> f32 {
    var x = input;
    x ^= x >> 16u; x *= 0x7feb352du; x ^= x >> 15u;
    x *= 0x846ca68bu; x ^= x >> 16u;
    return f32(x)/4294967295.0;
}

fn transition_blend(a: vec4<f32>, b: vec4<f32>, ratio: f32, layer: u32) -> vec4<f32> {
    let f = ratio/255.0;
    if layer==1u { return vec4(mix(a.rgb,b.rgb,f),1.0); }
    let alpha = mix(a.a,b.a,f);
    if layer==2u { return vec4(mix(a.rgb*a.a,b.rgb*b.a,f)/max(alpha,0.000001),alpha); }
    return mix(a,b,f);
}
fn transition_blur(t: texture_2d<f32>, xy: vec2<i32>, radius: vec2<i32>, bilinear: bool) -> vec4<f32> {
    if bilinear {
        let stride=radius*2+1;
        let lo=xy/stride*stride; let hi=min(lo+stride,vec2<i32>(textureDimensions(t))-1);
        let f=vec2<f32>(xy-lo)/max(vec2<f32>(hi-lo),vec2(1));
        return mix(mix(fetch(t,lo),fetch(t,vec2(hi.x,lo.y)),f.x),mix(fetch(t,vec2(lo.x,hi.y)),fetch(t,hi),f.x),f.y);
    }
    var sum=vec4(0.0);
    let lo=max(xy-radius,vec2(0)); let hi=min(xy+radius,vec2<i32>(textureDimensions(t))-1);
    for(var y=lo.y;y<=hi.y;y++) {
        var row=vec4(0.0);
        for(var x=lo.x;x<=hi.x;x++) { row+=round(fetch(t,vec2(x,y))*255.0); }
        sum+=floor(row/f32(hi.x-lo.x+1));
    }
    return floor(sum/f32(hi.y-lo.y+1))/255.0;
}

@compute @workgroup_size(8,8)
fn main(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let xy = vec2<i32>(invocation.xy)+vec2<i32>(max(p[0].xy,vec2(0)));
    let at = vec2<f32>(xy)+0.5;
    if any(xy>=vec2<i32>(textureDimensions(output))) || any(at>=p[0].zw) || !inside(at) { return; }
    let s = sample(source,coordinate(0u,at));
    let d = fetch(destination,vec2<i32>(floor(coordinate(1u,at))));
    let old = fetch(previous,xy);
    let color = p[5];
    let opacity = clamp(p[2].x/255.0,0.0,1.0);
    let op = u32(p[1].x);
    let kind = u32(p[1].y);
    var result = old;
    switch op {
        case 0u: { result = s; }
        case 1u: { result = color; }
        case 2u: { result = vec4(color.rgb,d.a); }
        case 3u: { result = vec4(d.rgb,opacity); }
        case 4u: { result = vec4(s.rgb,d.a); }
        case 5u: { result = vec4(d.rgb,s.a); }
        case 6u: { result = vec4(s.rgb,1.0); }
        case 7u: { result = over(d,s,opacity,kind); }
        case 8u: { result = over(d,vec4(s.rgb,1.0),opacity,kind); }
        case 9u: { result = over(d,vec4(color.rgb,1.0),opacity,kind); }
        case 10u: { result = over(d,vec4(color.rgb,s.r),opacity,kind); }
        case 11u: { result = vec4(d.rgb,d.a*(1.0-opacity)); }
        case 12u: { result = vec4(d.rgb,d.a*(1.0-s.r*abs(p[2].x)/255.0)); }
        case 13u: { result = mix(s,d,opacity); }
        case 14u: {
            let phase = p[2].z; let vague = max(p[2].w,1.0);
            let amount = clamp((phase-fetch(rule,vec2<i32>(floor(coordinate(2u,at)))).r*255.0)/vague,0.0,1.0);
            result = mix(s,d,amount);
        }
        case 15u: { result = vec4(d.rgb*d.a,d.a); }
        case 16u: { result = vec4(d.rgb/max(d.a,0.000001),d.a); }
        case 17u: { result = vec4(vec3(dot(d.rgb,vec3(0.299,0.587,0.114))),d.a); }
        case 18u: {
            var sum = vec4(0.0); var count = 0.0;
            for (var y=i32(p[3].y);y<=i32(p[3].w);y++) {
                for (var x=i32(p[3].x);x<=i32(p[3].z);x++) {
                    var v = fetch(previous,xy+vec2(x,y));
                    if (kind==1u) { v = vec4(v.rgb*v.a,v.a); }
                    sum += v; count += 1.0;
                }
            }
            result = sum/max(count,1.0);
            if (kind==1u) { result = vec4(result.rgb/max(result.a,0.000001),result.a); }
        }
        case 19u: {
            var rgb = d.rgb;
            if (kind==2u) { rgb /= max(d.a,0.000001); }
            rgb = p[3].xyz + pow(clamp(rgb,vec3(0),vec3(1)),1.0/max(p[4].xyz,vec3(0.000001)))*(p[5].xyz-p[3].xyz);
            if (kind==2u) { rgb *= d.a; }
            result = vec4(rgb,d.a);
        }
        case 20u: { result = vec4(mix(d.rgb,blend_color(d.rgb,s.rgb,u32(p[2].y)),s.a*opacity),d.a); }
        case 21u: {
            if (u32(p[2].y)==8u) {
                result = vec4(kirikiri_dodge(d.rgb,s.rgb,u32(clamp(p[2].x,0.0,255.0))),d.a);
            } else {
                result = vec4(mix(d.rgb,blend_color(d.rgb,s.rgb,u32(p[2].y)),opacity),d.a);
            }
        }
        case 35u: {
            let origin=xy-vec2<i32>(p[3].yz);
            var coverage=0.0; let radius=i32(ceil(p[3].x));
            for(var y=-radius;y<=radius;y++) { for(var x=-radius;x<=radius;x++) {
                if f32(x*x+y*y)<=p[3].x*p[3].x+0.5 {
                    let q=origin+vec2(x,y);
                    if all(q>=vec2(0)) && all(q<vec2<i32>(textureDimensions(source))) { coverage=max(coverage,fetch(source,q).a); }
                }
            } }
            let direction=p[4].zw-p[4].xy;
            var amount=0.0;
            if p[3].w!=0.0 {
                amount=dot(vec2<f32>(origin)-p[4].xy,direction)/max(dot(direction,direction),0.000001);
                let wrap=u32(p[3].z);
                if wrap==0u { amount=fract(amount); }
                else if wrap==1u { amount=1.0-abs((fract(amount*0.5)*2.0)-1.0); }
                else if wrap==2u { amount=fract(amount); }
                else if wrap==3u { amount=1.0-abs((fract(amount*0.5)*2.0)-1.0); }
                else { amount=clamp(amount,0.0,1.0); }
            }
            let ink=mix(p[2],p[5],amount);
            result=over(old,vec4(ink.rgb,ink.a*coverage),1.0,1u);
        }
        case 32u: {
            let q=vec2<i32>(floor(coordinate(0u,at)));
            let shift=i32(p[26u+u32(q.y)].x);
            let origin=q-vec2(shift,0);
            result=color;
            if origin.x>=0 && origin.x<i32(textureDimensions(source).x) {
                let q2=vec2<i32>(floor(coordinate(1u,at)))-vec2(shift,0);
                result=transition_blend(fetch(source,origin),fetch(destination,q2),p[2].x,u32(p[2].z));
            }
        }
        case 33u: {
            let size=max(p[3].x,2.0); let offset=p[3].yz;
            let q=floor(coordinate(0u,at));
            let origin=vec2<i32>(floor((q-offset)/size)*size+offset+floor(size/2.0));
            let q2=origin+vec2<i32>(p[7].xy-p[6].xy);
            result=mix(fetch(source,origin),fetch(destination,q2),opacity);
        }
        case 34u: {
            let a=transition_blur(source,vec2<i32>(floor(coordinate(0u,at))),vec2<i32>(p[3].xy),p[4].x==1.0);
            let b=transition_blur(destination,vec2<i32>(floor(coordinate(1u,at))),vec2<i32>(p[3].zw),p[4].x==1.0);
            result=mix(a,b,opacity);
        }
        case 31u: {
            let shift = i32(p[26u+u32(xy.y)].x);
            let origin = xy-vec2(shift,0);
            if f32(origin.x)>=p[0].x && f32(origin.x)<p[0].z { result = fetch(source,origin); }
        }
        case 30u: {
            let mask = fetch(source,vec2<i32>(floor(coordinate(0u,at)))+vec2(i32(p[3].x),0));
            result = vec4(s.rgb,mask.r);
        }
        case 23u: {
            let rgb = floor(d.rgb*255.0+0.5);
            result = vec4(clamp(trunc((rgb-128.0)*((100.0+trunc(p[3].y))/100.0)+trunc(p[3].x)+128.0),vec3(0),vec3(255))/255.0,d.a);
        }
        case 24u: {
            let rgb = vec3<i32>(floor(d.rgb*255.0+0.5));
            let hi = max(max(rgb.r,rgb.g),rgb.b);
            let lo = min(min(rgb.r,rgb.g),rgb.b);
            let l = ((hi+lo)*255+255)/510;
            let h = f32(i32(p[3].x)&255)*360.0/255.0;
            let sat = f32(i32(p[3].y)&255)/255.0;
            let colored = trunc(hsl_rgb(h,sat,f32(l)/255.0)*255.0);
            let a = p[3].z;
            var out = floor((colored*a+vec3<f32>(rgb)*(256.0-a))/256.0);
            if p[3].w != 0.0 { out = colored; }
            result = vec4(out/255.0,d.a);
        }
        case 25u: {
            let hi = max(max(d.r,d.g),d.b); let lo = min(min(d.r,d.g),d.b);
            let delta = hi-lo; let add = hi+lo;
            var l = add/2.0; var sat = 0.0; var h = 0.0;
            if delta != 0.0 {
                sat = delta/select(2.0-add,add,l<0.5);
                if d.r == hi { h = (d.g-d.b)/delta; }
                else if d.g == hi { h = 2.0+(d.b-d.r)/delta; }
                else { h = 4.0+(d.r-d.g)/delta; }
                h /= 6.0;
            }
            h = fract(h+trunc(p[3].x)/360.0);
            let ds = trunc(p[3].y)/100.0; let dl = trunc(p[3].z)/100.0;
            sat += select(sat,1.0-sat,ds>0.0)*ds;
            l += select(l,1.0-l,dl>0.0)*dl;
            result = vec4(trunc(hsl_rgb(h*360.0,sat,l)*255.0)/255.0,d.a);
        }
        case 26u,27u: {
            let seed = u32(xy.x)+u32(xy.y)*textureDimensions(output).x+u32(p[3].w)*747796405u;
            let n = vec3(hash_noise(seed),hash_noise(seed+0x1234567u),hash_noise(seed+0x7654321u));
            if op == 27u { result = vec4(vec3(floor(n.x*255.0)/255.0),d.a); }
            else { result = vec4(clamp(floor(d.rgb*255.0+0.5)+trunc((n-0.5)*trunc(p[3].x)),vec3(0),vec3(255))/255.0,d.a); }
        }
        case 28u,29u: {
            let length = i32(p[3].y); let mid = length/2;
            let axis = select(0,1,op==29u);
            let low = i32(p[0][axis]); let high = i32(p[0][axis+2]);
            let row = xy[axis]-low; let size = high-low;
            var sum = vec4(0.0); var scale = 0.0;
            for (var j=0;j<length;j++) {
                let position = row+j-mid;
                if position>=0 && position<size {
                    var at = xy; at[axis] = position+low;
                    let w = p[26+u32(j)].x;
                    scale += w;
                    // Upstream's small-image branch indexes the numerator by source row.
                    var numerator = w;
                    if length>size { numerator = p[26+u32(position)].x; }
                    sum += floor(fetch(previous,at)*255.0+0.5)*numerator;
                }
            }
            result = vec4<f32>(vec4<u32>(floor(sum/scale+0.5)) & vec4<u32>(255u))/255.0;
        }
        case 22u: {
            if (all(p[3] == vec4(0.0,1.0,0.0,1.0))) {
                result = s;
            } else {
                let y = dot(s.rgb,vec3(0.299,0.587,0.114));
                let cb = (s.b-y)/1.772;
                let cr = (s.r-y)/1.402;
                let angle = radians(p[3].z);
                let chroma = vec2(cb*cos(angle)-cr*sin(angle),cb*sin(angle)+cr*cos(angle))*p[3].w;
                let luma = y*p[3].y+p[3].x;
                result = vec4(luma+1.402*chroma.y,luma-0.344136*chroma.x-0.714136*chroma.y,luma+1.772*chroma.x,s.a);
            }
        }
        default: {}
    }
    textureStore(output,xy,clamp(result,vec4(0),vec4(1)));
}
