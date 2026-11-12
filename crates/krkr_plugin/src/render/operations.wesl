@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var destination: texture_2d<f32>;
@group(0) @binding(2) var rule: texture_2d<f32>;
@group(0) @binding(3) var previous: texture_2d<f32>;
@group(0) @binding(4) var output: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(5) var<storage, read> p: array<vec4<f32>>;
@group(0) @binding(6) var<storage, read_write> records: array<atomic<u32>>;
@group(0) @binding(7) var<storage, read> nagano_tables: array<vec4<f32>>;

const nagano_motion: u32 = 256u;

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

fn blend_channel(a:u32,b:u32,opa:u32,shift:u32)->u32{
 if shift==24u{return (a+(((b-a)*opa)>>8u))&255u;}
 let x=a<<shift;let y=b<<shift;return ((x+(((y-x)*opa)>>8u))>>shift)&255u;
}
fn nb(a:vec4<f32>,b:vec4<f32>,f:f32)->vec4<f32>{
 let aa=vec4<u32>(round(a*255.0));let bb=vec4<u32>(round(b*255.0));let opa=u32(max(f,0.0));
 return vec4<f32>(f32(blend_channel(aa.r,bb.r,opa,16u)),f32(blend_channel(aa.g,bb.g,opa,8u)),f32(blend_channel(aa.b,bb.b,opa,0u)),f32(blend_channel(aa.a,bb.a,opa,24u)))/255.0;
}
fn transition_blur(t: texture_2d<f32>, xy: vec2<i32>, radius: vec2<i32>, bilinear: bool) -> vec4<f32> {
    if bilinear {
        let stride=radius*2+1;
        let lo=xy/stride*stride; let hi=min(lo+stride,vec2<i32>(textureDimensions(t))-1);
        let f=vec2<i32>((xy-lo)*255/max(hi-lo,vec2(1)));
        let top=nb(fetch(t,lo),fetch(t,vec2(hi.x,lo.y)),f32(f.x));
        let bottom=nb(fetch(t,vec2(lo.x,hi.y)),fetch(t,hi),f32(f.x));
        return nb(top,bottom,f32(f.y));
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


fn ns(q:vec2<i32>,which:i32)->vec4<f32>{
 if which==0{return fetch(source,q+vec2<i32>(p[6].xy));}
 return fetch(destination,q+vec2<i32>(p[7].xy));
}
fn dark(c:vec4<f32>,f:i32)->vec4<f32>{let b=round(c*255.0);return vec4(floor(b.rg*f32(256-f)/256.0),b.b-floor(b.b*f32(f)/256.0),b.a)/255.0;}
fn light(c:vec4<f32>,f:i32)->vec4<f32>{let b=round(c*255.0);return vec4((b.rgb+floor((255.0-b.rgb)*f32(f)/256.0))/255.0,c.a);}
fn nagano(q:vec2<i32>)->vec4<f32>{
 let w=i32(p[3].x);let h=i32(p[3].y);let v=p[3].zw;let z=p[4];let tail=p[5].xy;
 let a=ns(q,0);let b=ns(q,1);let k=i32(p[2].x);
 if k==0 {
  let pos=i32(v.x);
  if (q.y&1)==0 {
   if q.x<pos{
    return ns(q+vec2(w-pos,0),1);
   }
   return ns(q-vec2(pos,0),0);
  }
  if q.x<w-pos{
   return ns(q+vec2(pos,0),0);
  }
  return ns(q-vec2(w-pos,0),1);
 } // TODO: 同步bug
 if k==1 {let center=vec2<f32>(f32(w),f32(h))*0.5;let aa=ns(clamp(vec2<i32>(center+(vec2<f32>(q)-center)*v.y),vec2(0),vec2(w-1,h-1)),0);let bb=ns(clamp(vec2<i32>(center+(vec2<f32>(q)-center)*z.x),vec2(0),vec2(w-1,h-1)),1);if bb.a==0.0{return vec4(aa.rgb,floor(round(aa.a*255.0)*(255.0-v.x)/256.0)/255.0);}if aa.a==0.0{return vec4(bb.rgb,floor(round(bb.a*255.0)*v.x/256.0)/255.0);}return nb(aa,bb,v.x);}
 if k==2 {let f=vec4(v,z.xy);var c=floor(round(a*255.0)+(round(b*255.0)-round(a*255.0))*f/256.0);if a.a==0.0||b.a==0.0 {c=vec4<f32>(vec3<f32>(vec3<i32>(round(a.rgb*255.0)*f.rgb)&vec3(255)),floor(round(b.a*255.0)*f.a/256.0));if b.a==0.0{c.a=round(a.a*255.0)-floor(round(a.a*255.0)*f.a/256.0);}}return c/255.0;}
 if k==3 {let r=p[26u+u32(q.x)];let sy=((i32(r.w)*(q.y-h/2))>>8)+h/2;if r.x<0.0||sy<0||sy>=h{return vec4(0);}let selected=i32(r.x);if selected==0 {return ns(vec2<i32>(i32(r.y),sy),0);}if selected==1 {return ns(vec2<i32>(i32(r.z),sy),1);}return vec4(0.0);}
 if k==5 {
 let phase=i32(v.x);if phase==0{return a;}if phase==w/2{return b;}
 let mp=min(phase,16);let shade=i32(sqrt(f32(max(0,(((w/2-phase)<<16)/w)/2)))+0.5);let hg=shade/2;let x=q.x;
 var f=0;var sc=0;var mode=0;
 if v.y==0.0 {
  let b1=clamp(w-2*phase-mp,0,w);let b2=clamp(w-2*phase,0,w);let b3=clamp(w-3*phase/2,0,w);let b4=clamp(w-phase,0,w);
  if x<b1{return a;}if x<b2{return dark(a,((x-b2+mp)*shade/mp)/2);}if x<b3{f=(b3-x)*shade*2/phase;mode=1;}else if x<b4{f=((x-b4)*2+phase)*shade/phase;mode=2;}else{return dark(b,(w-x)*shade/phase);}sc=x+2*phase-w;
 }else{
  let b1=clamp(phase,0,w);let b2=clamp(3*phase/2,0,w);let b3=clamp(2*phase,0,w);let b4=clamp(2*phase+mp,0,w);
  if x<b1{return dark(b,(b1-x)*shade/phase);}if x<b2{f=(b2-x)*shade*2/phase;mode=2;}else if x<b3{f=(phase/2+(x-b3)*2)*shade/phase;mode=1;}else if x<b4{return dark(a,((b4-x)*shade/mp)/2);}else{return a;}sc=x+w-2*phase;
 }
 var c=vec4(0.0);if sc>=0&&sc<w{c=ns(vec2(sc,q.y),1);}var top=dark(c,max(0,f));if mode==1{top=light(c,max(0,f));}return vec4(nb(dark(a,hg),top,round(c.a*255.0)).rgb,a.a);
 }
 if k==4 {
 let phase=i32(v.x);let cs=i32(v.y);if phase==0{return a;}if phase==w+h{return b;}
 let rb=min(w,h-q.y-phase+w);let base=h-phase-cs;let mid=base+8;if q.y<cs&&q.x<max(w-phase+8,0){return b;}var c=b;
 if q.y<cs{c=b;}else if q.x>=cs&&q.x<rb{c=ns(q-vec2(cs),0);}
 if q.y>max(base,0)&&q.y<=mid {if q.x>=max(w-phase+12,0)&&q.x<rb{c=nb(c,vec4(0,0,0,1),f32(((q.y-max(base,0))*63/8)&255));}}
 else if q.y>mid&&q.y<=mid+w {
  let cb=w-phase+8;let start=max(cb,0);var lc=h-cs-1;if cb<0{lc+=cb;}let end=min(rb,lc+start);
  if q.x>=start&&q.x<end {let index=clamp(q.y-mid,0,w-1)*h-cb+q.x;let bp=fetch(source,vec2(index/h,h-1-index%h));let back=nb(bp,vec4(z.zw,tail.x,255.0)/255.0,z.y);c=nb(c,back,z.x);}
  if q.x>=max(start,end-24)&&q.x<end-16{c=nb(c,vec4(1),f32((q.x-(end-16))*127/8+127));}
  if q.x>=max(start,end-16)&&q.x<end-8{c=nb(c,vec4(1),f32((end-8-q.x)*127/8));}
  if q.x>=max(start,end-8)&&q.x<end{c=nb(c,vec4(0,0,0,1),f32((q.x-end)*255/8+255));}
 }return c;
 }
 if k==6 {let edge=i32(p[26u+u32(q.y)].x);let threshold=i32(v.x);let left=select(b,a,v.y!=0.0);let right=select(a,b,v.y!=0.0);if q.x<threshold{return left;}if q.x<threshold+edge {let rp=fetch(rule,vec2(q.x-threshold,q.y));return nb(left,rp,round(rp.a*255.0));}return right;}
 if k==7 {
  let size=max(v.y,4.0);let s=size;let root3=1.7320508075688772;let halfW=s*root3*0.5;
  let aq=(root3/3.0*f32(q.x)-f32(q.y)/3.0)/s;let ar=(2.0/3.0*f32(q.y))/s;
  var rx=floor(aq+0.5);var ry=floor(-aq-ar+0.5);var rz=floor(ar+0.5);
  let pixel=u32(q.y*w+q.x);let axes=p[26u+pixel/4u];let axis=u32(axes[pixel&3u]);
  if axis==1u {rx=-ry-rz;} else if axis==2u {ry=-rx-rz;} else {rz=-rx-ry;}
  let cx=i32(s*root3*(rx+rz*0.5)+0.5);let cy=i32(s*1.5*rz+0.5);
  let ady=abs(f32(q.y)-s*1.5*rz);var hh=0.0;if ady<=s*0.5 {hh=halfW;}else if ady<=s {hh=halfW*(s-ady)/(s*0.5);}
  let half=max(i32(hh+0.5),1);let order=i32(z.y);let dir=i32(z.z);
  var odx=0.0;var ody=1.0;var radial=false;
  if order==1 {odx=-1.0;}else if order==2 {odx=0.0;}else if order==3 {odx=1.0;}else if order==4 {odx=-1.0;ody=0.0;}else if order==5 {radial=true;}else if order==6 {odx=1.0;ody=0.0;}else if order==7 {odx=-1.0;ody=-1.0;}else if order==8 {odx=0.0;ody=-1.0;}else if order==9 {odx=1.0;ody=-1.0;}
  let nx=f32(cx)/max(f32(w-1),1.0);let ny=f32(cy)/max(f32(h-1),1.0);let naxis=max(select(0.0,1.0,odx!=0.0)+select(0.0,1.0,ody!=0.0),1.0);
  var delayf=0.0;if radial {let ex=(nx-0.5)*2.0;let ey=(ny-0.5)*2.0;delayf=min(length(vec2(ex,ey))/1.4142135623730951,1.0);}else{let px=select(select(0.0,nx,odx>0.0),1.0-nx,odx<0.0);let py=select(select(0.0,ny,ody>0.0),1.0-ny,ody<0.0);delayf=(px+py)/naxis;}
  let delay=clamp(i32(delayf*192.0+0.5),0,192);let lp=i32(v.x)-delay;
  if lp<=0 {return a;}if lp>=63 {return b;}
  var src=0;var vis=0.0;if lp<32 {vis=f32(32-lp)/32.0;}else{src=1;vis=f32(lp-31)/32.0;}
  let visHalf=f32(half)*vis;if visHalf<0.5{return vec4(0.0);}
  var rel=f32(q.x-cx);if dir==1||dir==4||dir==7 {rel=-rel;}if rel < -visHalf||rel>visHalf{return vec4(0.0);}
  var srcRel=rel*f32(half)/visHalf+f32(i32(z.x))/max(size,1.0)*f32(q.y-cy)*(1.0-vis);
  let srcX=cx+i32(srcRel+select(-0.5,0.5,srcRel>=0.0));if srcX<0||srcX>=w{return vec4(0.0);}
  return ns(vec2(srcX,q.y),src);
 }
 if k==8 {
  let pixels=u32(z.x);let base=u32(q.y*w+q.x);let next_base=pixels;let code_base=pixels*3u;
  var c=vec4(0.0);if v.y>=0.0 {c=ns(q,i32(v.y));}
  let phase=i32(v.x);
  var node=atomicLoad(&records[base]);
  loop {
   if node==0u {break;}
   let code=atomicLoad(&records[code_base+node-1u]);
   let c0=code-1u;let src=i32(c0/u32(pixels));let index=i32(c0%u32(pixels));
   let sq=vec2(index%w,index/w);let rp=fetch(rule,sq);let start=i32(round(rp.b*255.0));
   let localPhase=select(255-phase,phase,src==0);let sv=ns(sq,src);
   if localPhase<start {c=sv;} else {c=nb(c,sv,f32(max((localPhase-start)*255/max(256-start,1),0)));}
   node=atomicLoad(&records[next_base+node-1u]);
  }
  return c;
 }
 if k==9 {
  let count=i32(v.x);let base=i32(v.y);let roundness=max(z.x,0.000001);let len=i32(z.y);var blend=base;var vd=0;
  for(var i=0;i<count;i++) {let r=p[26u+u32(i)*2u];if i32(r.w)!=1 {continue;}let dx=abs(q.x-i32(r.x));let dy=abs(q.y-i32(r.y));let distance=min(i32(floor(sqrt(f32(dx*dx)+f32(dy*dy)*roundness*roundness)+0.5)),65535);var wc=i32(r.z)-distance;if wc>=0 {wc=min(wc,len-1);let table=p[26u+u32(count)*2u+u32(wc)];let drift=p[26u+u32(i)*2u+1u].x;blend+=i32(table.y);vd-=(i32(table.x)*i32(drift))>>8;}}
  let aa=ns(vec2(q.x,clamp(q.y+vd,0,h-1)),0);let bb=ns(vec2(q.x,clamp(q.y+vd,0,h-1)),1);if (blend&256)!=0 {return vec4(nb(bb,aa,f32(blend&255)).rgb,1.0);}return vec4(nb(aa,bb,f32(blend&255)).rgb,1.0);
 }
 if k==10 {
  let triangleCount=i32(p[3].w);var mappedA=q;var mappedB=q;
  for(var tri=0;tri<triangleCount;tri++) {
   let j=26u+u32(tri)*6u;let ca=p[j].xy;let cb=p[j+2u].xy;let cc=p[j+4u].xy;
   let a=vec2<i32>(ca);let b=vec2<i32>(cb);let c=vec2<i32>(cc);
   let den=(b.y-c.y)*(a.x-c.x)+(c.x-b.x)*(a.y-c.y);if den==0 {continue;}
   let w0=(b.y-c.y)*(q.x-c.x)+(c.x-b.x)*(q.y-c.y);
   let w1=(c.y-a.y)*(q.x-c.x)+(a.x-c.x)*(q.y-c.y);let w2=den-w0-w1;
   if den>0 {if w0<0||w1<0||w2<0 {continue;}} else {if w0>0||w1>0||w2>0 {continue;}}
   let fw0=f32(w0)/f32(den);let fw1=f32(w1)/f32(den);let fw2=f32(w2)/f32(den);
   let ba=p[j].zw;let bb=p[j+2u].zw;let bc=p[j+4u].zw;let da=p[j+1u].xy;let db=p[j+3u].xy;let dc=p[j+5u].xy;
   mappedA=clamp(vec2<i32>(floor(fw0*ba+fw1*bb+fw2*bc+0.5)),vec2(0),vec2(w-1,h-1));mappedB=clamp(vec2<i32>(floor(fw0*da+fw1*db+fw2*dc+0.5)),vec2(0),vec2(w-1,h-1));
  }
  return nb(ns(mappedA,0),ns(mappedB,1),f32(v.x));
 }
 return a;
}

fn nagano_record(destination:u32,code:u32,pixels:u32) {
 let next_base=pixels;let code_base=pixels*3u;let counter=pixels*5u;
 let node=atomicAdd(&records[counter],1u)+1u;
 atomicStore(&records[code_base+node-1u],code);
 loop {
  let first=atomicLoad(&records[destination]);
  if first==0u||atomicLoad(&records[code_base+first-1u])>code {
   atomicStore(&records[next_base+node-1u],first);
   let inserted=atomicCompareExchangeWeak(&records[destination],first,node);
   if inserted.exchanged {return;}
   continue;
  }
  var previous=first;
  var following=atomicLoad(&records[next_base+previous-1u]);
  while following!=0u&&atomicLoad(&records[code_base+following-1u])<code {
   previous=following;
   following=atomicLoad(&records[next_base+previous-1u]);
  }
  atomicStore(&records[next_base+node-1u],following);
  let inserted=atomicCompareExchangeWeak(&records[next_base+previous-1u],following,node);
  if inserted.exchanged {return;}
 }
}

@compute @workgroup_size(8,8)
fn nagano_scatter(@builtin(global_invocation_id) invocation: vec3<u32>) {
 let q=vec2<i32>(invocation.xy);
 let w=i32(p[3].x);let h=i32(p[3].y);let phase=i32(p[3].z);let mask=u32(p[4].y);
 if any(q>=vec2(w,h)) {return;}
 let rp=fetch(rule,q);
 let start=i32(round(rp.b*255.0));
 let dir=i32(round(rp.r*255.0));let spd=i32(round(rp.g*255.0));
 let trig=nagano_tables[u32(dir)].xy;let pixels=u32(p[4].x);let index=u32(q.y*w+q.x);
 for(var step=0u;step<2u;step++) {
  if (mask&(1u<<step))==0u {continue;}
  let ph=select(255-phase,phase,step==0u);
  var nx=q.x;var ny=q.y;
  if ph>=start {
   let amount=i32(nagano_tables[nagano_motion+u32(ph-start)][step]);
   nx+=(i32(trig.x)*spd*amount)>>19;ny+=(i32(trig.y)*spd*amount)>>19;
  }
  if nx<0||nx>=w||ny<0||ny>=h {continue;}
  nagano_record(u32(ny*w+nx),step*pixels+index+1u,pixels);
 }
}

@compute @workgroup_size(8,8)
fn main(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let xy = vec2<i32>(invocation.xy)+vec2<i32>(max(p[0].xy,vec2(0)));
    let at = vec2<f32>(xy)+0.5;
    if any(xy>=vec2<i32>(textureDimensions(output))) || any(at>=p[0].zw) || !inside(at) { return; }
    let operation = u32(p[1].x);
    if operation==40u || operation==41u {
        let radius = i32(p[3][select(0u,1u,operation==41u)]);
        let delta = select(vec2(radius,0),vec2(0,radius),operation==41u);
        let lo=max(xy-delta,vec2(0));let hi=min(xy+delta,vec2<i32>(textureDimensions(source))-1);
        var sum=vec4(0.0);
        if operation==40u {
            for(var x=lo.x;x<=hi.x;x++){sum+=round(fetch(source,vec2(x,xy.y))*255.0);}
            textureStore(output,xy,floor(sum/f32(hi.x-lo.x+1))/255.0);
        } else {
            for(var y=lo.y;y<=hi.y;y++){sum+=round(fetch(source,vec2(xy.x,y))*255.0);}
            textureStore(output,xy,floor(sum/f32(hi.y-lo.y+1))/255.0);
        }
        return;
    }
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
            let srcq=vec2<i32>(floor(coordinate(0u,at)));
            let dstq=vec2<i32>(floor(coordinate(1u,at)));
            let pos=clamp(i32(p[26u+u32(srcq.y)].x),0,i32(p[3].x));
            let width=i32(p[3].x);
            if (srcq.y&1)==0 {
                if srcq.x<pos { result=fetch(destination,dstq+vec2(width-pos,0)); }
                else { result=fetch(source,srcq-vec2(pos,0)); }
            } else {
                if srcq.x<width-pos { result=fetch(source,srcq+vec2(pos,0)); }
                else { result=fetch(destination,dstq-vec2(width-pos,0)); }
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
            var a:vec4<f32>;var b:vec4<f32>;
            if p[4].y==1.0 {
                a=fetch(source,vec2<i32>(floor(coordinate(0u,at))));
                b=fetch(destination,vec2<i32>(floor(coordinate(1u,at))));
            } else {
                a=transition_blur(source,vec2<i32>(floor(coordinate(0u,at))),vec2<i32>(p[3].xy),true);
                b=transition_blur(destination,vec2<i32>(floor(coordinate(1u,at))),vec2<i32>(p[3].zw),true);
            }
            result=nb(a,b,p[2].x);
        }
        case 36u: {
            let logical=xy-vec2<i32>(p[0].xy)+vec2<i32>(p[2].zw);
            let tile=logical/64;
            let phase=clamp(i32(p[3].x)-(tile.x-tile.y)*2,0,63);
            let local=logical-tile*64;
            var q=logical;
            var covered=true;
            if phase!=0 && phase!=63 {
                let j=26u+u32((phase*64+local.y)*2);
                let row=bitcast<vec4<i32>>(p[j]);
                let step=bitcast<vec4<i32>>(p[j+1u]);
                covered=local.x>=row.x && local.x<row.x+row.y;
                q=tile*64+((row.zw+(local.x-row.x)*step.xy)>>vec2<u32>(16));
                covered=covered && all(q>=vec2(0)) && all(q<vec2<i32>(p[3].yz));
            }
            result=color;
            if covered {
                if phase<32 { result=fetch(source,q+vec2<i32>(p[6].xy-p[2].zw)); }
                else { result=fetch(destination,q+vec2<i32>(p[7].xy-p[2].zw)); }
                let gloss=array(0,0,0,0,16,48,80,128,192,128,80,48,16);
                if phase<13 {
                    let amount=f32(gloss[u32(phase)]);
                    let bytes=round(result*255.0);
                    result=floor(bytes+(vec4(255.0,255.0,255.0,0.0)-bytes)*amount/256.0)/255.0;
                }
            }
        }
        case 37u: {
            let logical=xy-vec2<i32>(p[0].xy)+vec2<i32>(p[2].zw);
            result=color;
            if logical.y>=0 && logical.y<i32(p[3].z) {
                let j=26u+u32(logical.y)*10u;
                let count=bitcast<vec4<i32>>(p[j]).x;
                for(var i=0;i<count;i++) {
                    let region=bitcast<vec4<i32>>(p[j+5u+u32(i)]);
                    if logical.x<region.x || logical.x>=region.y || region.z==0 { continue; }
                    let k=j+1u+u32(region.z-1)*2u;
                    let row=bitcast<vec4<i32>>(p[k]);
                    let stepy=bitcast<vec4<i32>>(p[k+1u]).x;
                    let q=(row.yz+(logical.x-row.x)*vec2(row.w,stepy))>>vec2<u32>(16);
                    if all(q>=vec2(0)) && all(q<vec2<i32>(p[3].yz)) {
                        if region.z==1 { result=fetch(source,q+vec2<i32>(p[6].xy-p[2].zw)); }
                        else { result=fetch(destination,q+vec2<i32>(p[7].xy-p[2].zw)); }
                    }
                }
            }
        }
        case 38u: {
            let f=opacity; let center=vec2(p[3].yz); let delta=at-center;
            let distance=length(vec2(delta.x,delta.y/max(p[4].x,0.0001)));
            let envelope=sin(clamp(distance/max(f32(textureDimensions(source).x),1.0),0.0,1.0)*3.14159265);
            let phase=distance/max(p[3].w,1.0)*6.2831853-p[4].y*f*6.2831853;
            let drift=sin(phase)*envelope*p[4].z*sin(3.14159265*f);
            let direction=normalize(delta+vec2(0.0001));
            let mapped=vec2<i32>(at+direction*drift);
            result=mix(fetch(source,mapped),fetch(destination,mapped),f);
        }
        case 39u: {
            result=nagano(xy-vec2<i32>(p[0].xy)+vec2<i32>(p[2].zw));
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
