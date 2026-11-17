@group(0) @binding(0) var<storage, read> data: array<vec4<f32>>;
@group(0) @binding(1) var output: texture_storage_2d<rgba8unorm, write>;
fn extend_axis(value: f32, origin: f32, size: f32, reflect: bool) -> f32 {
    let t = (value-origin)/size;
    if reflect { return origin + (1.0-abs((t*0.5-floor(t*0.5))*2.0-1.0))*size; }
    return origin + (t-floor(t))*size;
}
fn ramp(t: f32) -> vec4<f32> {
    let n=u32(data[0].w);
    var a=data[8]; var pa=data[9].x;
    for(var i=1u;i<n;i++) {
        let b=data[8u+2u*i]; let pb=data[9u+2u*i].x;
        if t <= pb || i+1u==n {
            let f=clamp((t-pa)/max(pb-pa,0.000001),0.0,1.0);
            if data[7].w != 0.0 {
                let rgb=pow(mix(pow(a.rgb,vec3(2.2)),pow(b.rgb,vec3(2.2)),f),vec3(1.0/2.2));
                return vec4(rgb,mix(a.a,b.a,f));
            }
            return mix(a,b,f);
        }
        a=b; pa=pb;
    }
    return a;
}
fn cross2(a:vec2<f32>,b:vec2<f32>)->f32 { return a.x*b.y-a.y*b.x; }
@compute @workgroup_size(8,8)
fn main(@builtin(global_invocation_id) gid:vec3<u32>) {
    if any(gid.xy>=textureDimensions(output)) { return; }
    let pos=vec2<f32>(gid.xy)+0.5;
    var q=vec2(dot(data[1].xy,pos)+data[1].z,dot(data[2].xy,pos)+data[2].z);
    let tile=data[3]; let wrap=u32(data[0].z);
    if wrap < 4u {
        q.x=extend_axis(q.x,tile.x,tile.z,wrap==1u || wrap==3u);
        q.y=extend_axis(q.y,tile.y,tile.w,wrap==2u || wrap==3u);
    }
    var c=vec4(0.0);
    if data[0].y == 4.0 {
        let v=data[4].zw-data[4].xy;
        let t=dot(q-data[4].xy,v)/dot(v,v);
        c=ramp(clamp(t,0.0,1.0));
    } else {
        let center=data[4].xy;
        let d=q-center;
        var outer=1e20; var inner=0.0; var edge=vec4(0.0);
        let n=u32(data[5].z); let base=8u+2u*u32(data[0].w);
        for(var i=0u;i<n;i++) {
            let j=(i+1u)%n;
            let a=data[base+2u*i].xy; let b=data[base+2u*j].xy;
            let e=b-a; let det=cross2(d,e);
            if abs(det)>0.000001 {
                let t=cross2(a-center,e)/det; let u=cross2(a-center,d)/det;
                if t>0.0 && u>=0.0 && u<=1.0 && t<outer {
                    outer=t; edge=mix(data[base+2u*i+1u],data[base+2u*j+1u],u);
                }
            }
            let ia=center+(a-center)*data[5].xy; let ib=center+(b-center)*data[5].xy;
            let ie=ib-ia; let idet=cross2(d,ie);
            if abs(idet)>0.000001 {
                let t=cross2(ia-center,ie)/idet; let u=cross2(ia-center,d)/idet;
                if t>0.0 && u>=0.0 && u<=1.0 { inner=max(inner,t); }
            }
        }
        var t=0.0;
        if dot(d,d)>0.000001 { t=clamp((1.0-inner)/max(outer-inner,0.000001),0.0,1.0); }
        let blend=ramp(t);
        if data[7].z!=0.0 { c=blend; }
        else { c=mix(data[6],edge,blend.r); }
    }
    textureStore(output,vec2<i32>(gid.xy),vec4(c.rgb*c.a,c.a));
}
