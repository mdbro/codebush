struct Camera { viewport:vec2<f32>,center:vec2<f32>,offset:vec2<f32>,zoom:f32,dpi:f32 };
fn linear_rgb(c:vec3<f32>)->vec3<f32>{return select(pow((c+0.055)/1.055,vec3(2.4)),c/12.92,c<=vec3(0.04045));}
@group(0) @binding(0) var<uniform> camera:Camera;
@group(0) @binding(1) var atlas:texture_2d<f32>;
@group(0) @binding(2) var sampler_atlas:sampler;
struct Projection { u:array<vec4<f32>,3>,v:array<vec4<f32>,3>,weights:array<vec4<f32>,17>,flags:vec4<u32> };
struct Node { p:array<vec4<f32>,3>,bounds:vec4<f32>,params:vec4<f32>,mask:vec4<u32> };
@group(1) @binding(0) var<uniform> projection:Projection;
@group(1) @binding(1) var<storage,read> nodes:array<Node>;
struct FileIndex { id:vec4<u32> };
@group(2) @binding(0) var<uniform> file_index:FileIndex;
fn weight(p:u32)->f32 {return projection.weights[p/4u][p%4u];}
fn visibility(n:Node)->f32 {var a=0.;for(var word=0u;word<3u;word++){var bits=n.mask[word];while bits!=0u {let bit=countTrailingZeros(bits);a=max(a,weight(word*32u+bit));bits=bits&(bits-1u);}}if a<=1e-6{return 0.;}return pow(a,0.25);}
fn anchor(n:Node)->vec2<f32>{var p=vec2(0.);for(var i=0u;i<3u;i++){p+=vec2(dot(n.p[i],projection.u[i]),dot(n.p[i],projection.v[i]));}return p;}
fn clip_relative(delta:vec2<f32>)->vec4<f32>{let p=(delta*camera.zoom+camera.offset)*camera.dpi;return vec4(p.x/camera.viewport.x*2.-1.,1.-p.y/camera.viewport.y*2.,0.,1.);}
fn clip(world:vec2<f32>)->vec4<f32>{return clip_relative(world-camera.center);}
fn corner(vi:u32)->vec2<f32>{let c=array<vec2<f32>,6>(vec2(0.,0.),vec2(1.,0.),vec2(0.,1.),vec2(0.,1.),vec2(1.,0.),vec2(1.,1.));return c[vi];}
struct Input{@location(0) rect:vec4<f32>,@location(1) uv:vec4<f32>,@location(2) color:vec4<f32>};
struct CardInput{@location(0) rect:vec4<f32>,@location(1) uv:vec4<f32>,@location(2) color:vec4<f32>,@location(3) id:u32};
struct Output{@builtin(position) position:vec4<f32>,@location(0) uv:vec2<f32>,@location(1) color:vec4<f32>,@location(2) local:vec2<f32>,@location(3) @interpolate(flat) shape:vec3<f32>,@location(4) @interpolate(flat) pixel_height:f32};
fn make(rect:vec4<f32>,uv:vec4<f32>,color:vec4<f32>,id:u32,vi:u32)->Output {
 let n=nodes[id];let c=corner(vi);let p=rect.xy+c*rect.zw;
 var o:Output;o.position=clip_relative((anchor(n)-camera.center)+(p-n.bounds.xy-n.bounds.zw*0.5)*n.params.x);
 let visible=visibility(n);let a=select(visible,smoothstep(0.0316228,0.1,visible),projection.flags.x==id && visible>0.);if a<0.001 {o.position=vec4(2.,2.,0.,1.);}
 o.uv=uv.xy+c*uv.zw;o.color=vec4(color.rgb,color.a*a);o.local=(c-0.5)*rect.zw;
 o.shape=vec3(rect.zw*0.5,select(-1.,uv.y,uv.x<0.));o.pixel_height=rect.w*n.params.x*camera.zoom*camera.dpi;return o;
}
@vertex fn source_vs(i:Input,@builtin(vertex_index) vi:u32)->Output{return make(i.rect,i.uv,i.color,file_index.id.x,vi);}
@vertex fn card_vs(i:CardInput,@builtin(vertex_index) vi:u32)->Output{
 var o=make(i.rect,i.uv,i.color,i.id,vi);let n=nodes[i.id];
 o.pixel_height=14.*n.params.z*n.params.x*camera.zoom*camera.dpi;
 if camera.zoom*camera.dpi*n.params.x>n.params.y {o.position=vec4(2.,2.,0.,1.);}return o;
}
@fragment fn source_fs(i:Output)->@location(0) vec4<f32>{
 var a=textureSample(atlas,sampler_atlas,i.uv).r;
 if i.shape.z>=0. {let r=min(i.shape.z,min(i.shape.x,i.shape.y));let q=abs(i.local)-i.shape.xy+r;let d=length(max(q,vec2(0.)))+min(max(q.x,q.y),0.)-r;let aa=max(fwidth(d),0.001);a=1.-smoothstep(-aa,0.,d);}
 if i.shape.z<0. && dot(i.color.rgb,vec3(0.2126,0.7152,0.0722))<0.5 {a=pow(a,mix(1.,0.42,1.-smoothstep(2.,10.,i.pixel_height)));}
 return vec4(linear_rgb(i.color.rgb),i.color.a*a);
}
@fragment fn card_fs(i:Output)->@location(0) vec4<f32>{
 let c=textureSample(atlas,sampler_atlas,i.uv);var color=c.rgb;
 let size=vec2<f32>(textureDimensions(atlas));
 let footprint=max(length(dpdx(i.uv)*size),length(dpdy(i.uv)*size));
 let filtered=smoothstep(1.,3.,footprint);
 let paper=linear_rgb(i.color.rgb);let luma=vec3(0.2126,0.7152,0.0722);
 if dot(paper,luma)>0.5 {
  let deficit=dot(paper-color,luma);
  let small=1.-smoothstep(2.,10.,i.pixel_height);
  let coverage=smoothstep(0.,0.025,deficit)*smoothstep(0.10,0.35,dot(color,luma));
  color=max(vec3(0.),paper-(paper-color)*(1.+2.5*small*coverage*filtered));
 }
 return vec4(color,c.a*i.color.a);
}
struct EdgeInput{@location(0) data:vec4<u32>};
struct EdgeOutput{@builtin(position) position:vec4<f32>,@location(0) color:vec4<f32>};
@vertex fn edge_vs(i:EdgeInput,@builtin(vertex_index) vi:u32)->EdgeOutput{
 let na=nodes[i.data.x];let nb=nodes[i.data.y];let a=anchor(na);let b=anchor(nb);let delta=b-a;
 let len=max(length(delta),0.001);let unit=delta/len;let perp=vec2(-unit.y,unit.x);
 let incident=projection.flags.x==i.data.x || projection.flags.x==i.data.y;
 let c=corner(vi%6u);var p=vec2(0.);
 if vi<6u {p=mix(a,b,c.x)+perp*(c.y-0.5)*select(1.,2.2,incident)/camera.zoom;}
 else {let tip=mix(a,b,0.62);let tri=array<vec2<f32>,3>(vec2(0.,0.),vec2(-7.,-3.4),vec2(-7.,3.4));let t=tri[vi-6u]/camera.zoom;p=tip+unit*t.x+perp*t.y;}
 var o:EdgeOutput;o.position=clip(p);let w=weight(i.data.z);if w<=1e-6 || i.data.w==0u {o.position=vec4(2.,2.,0.,1.);}
 if vi>=6u && !incident && camera.zoom<0.04 {o.position=vec4(2.,2.,0.,1.);}
 let alpha=select(select(0.16,0.06,projection.flags.x!=0xffffffffu),1.,incident);
 o.color=vec4(linear_rgb(select(vec3(0.43,0.82,0.77),vec3(0.82,1.,0.91),incident)),pow(w,0.25)*alpha);return o;
}
@fragment fn edge_fs(i:EdgeOutput)->@location(0) vec4<f32>{return i.color;}
