struct Camera { viewport: vec2<f32>, center: vec2<f32>, offset: vec2<f32>, zoom: f32, dpi: f32 };
@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var atlas: texture_2d<f32>;
@group(0) @binding(2) var atlas_sampler: sampler;
struct In { @location(0) rect: vec4<f32>, @location(1) uv: vec4<f32>, @location(2) color: vec4<f32> };
struct Out { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32>, @location(1) color: vec4<f32>, @location(2) local: vec2<f32>, @location(3) @interpolate(flat) shape: vec3<f32>, @location(4) @interpolate(flat) pixel_height: f32 };
fn atlas_vertex(input: In, vi: u32) -> Out {
 let corners = array<vec2<f32>,6>(vec2(0.,0.),vec2(1.,0.),vec2(0.,1.),vec2(0.,1.),vec2(1.,0.),vec2(1.,1.));
 let p=corners[vi]; let world=input.rect.xy+p*input.rect.zw;
 let screen=((world-camera.center)*camera.zoom+camera.offset)*camera.dpi;
 var out:Out; out.position=vec4(screen.x/camera.viewport.x*2.-1.,1.-screen.y/camera.viewport.y*2.,0.,1.);
 out.uv=input.uv.xy+p*input.uv.zw;out.color=input.color;out.local=(p-0.5)*input.rect.zw;
 out.shape=vec3(input.rect.zw*0.5,select(-1.,input.uv.y,input.uv.x<0.));out.pixel_height=input.rect.w*camera.zoom*camera.dpi;return out;
}
@vertex fn vs(input: In, @builtin(vertex_index) vi: u32) -> Out {return atlas_vertex(input,vi);}
@fragment fn fs(input:Out)->@location(0) vec4<f32> {
 let coverage=textureSample(atlas,atlas_sampler,input.uv).r;
 var a=coverage;
 if input.shape.z>=0. {let radius=min(input.shape.z,min(input.shape.x,input.shape.y));let q=abs(input.local)-input.shape.xy+radius;let d=length(max(q,vec2(0.)))+min(max(q.x,q.y),0.)-radius;let aa=max(fwidth(d),0.001);a=1.-smoothstep(-aa,0.,d);}
 // Optical coverage correction keeps dark source strokes visible on paper
 // when minified. Reading-scale glyphs and every solid surface are unchanged.
 if input.shape.z<0. && dot(input.color.rgb,vec3(0.2126,0.7152,0.0722))<0.5 {
  let small=1.-smoothstep(2.,10.,input.pixel_height);
  a=pow(a,mix(1.,0.42,small));
 }
 return vec4(linear_rgb(input.color.rgb),input.color.a*a);
}

@fragment fn fs_cache(input:Out)->@location(0) vec4<f32> { return textureSample(atlas,atlas_sampler,input.uv); }

@vertex fn vs_cache_display(input:In,@builtin(vertex_index) vi:u32)->Out {
 var out=atlas_vertex(input,vi);out.pixel_height=input.color.a*camera.zoom*camera.dpi;return out;
}
@fragment fn fs_cache_display(input:Out)->@location(0) vec4<f32> {
 let c=textureSample(atlas,atlas_sampler,input.uv);
 let size=vec2<f32>(textureDimensions(atlas));
 let footprint=max(length(dpdx(input.uv)*size),length(dpdy(input.uv)*size));
 if input.color.a<0. {return c;}
 return vec4(paper_ink(c.rgb,linear_rgb(input.color.rgb),input.pixel_height,smoothstep(1.,3.,footprint)),c.a);
}
// Restore the contrast of subpixel source patterns after mip filtering. The
// actual file background is supplied per panel; empty paper remains unchanged.
// Apply only at display time, never recursively while constructing mip levels.
fn paper_ink(c:vec3<f32>,paper:vec3<f32>,pixels:f32,filtered:f32)->vec3<f32> {
 let luma=vec3(0.2126,0.7152,0.0722);
 let deficit=dot(paper-c,luma);
 let small=1.-smoothstep(2.,10.,pixels);
 let coverage=smoothstep(0.,0.025,deficit)*smoothstep(0.10,0.35,dot(c,luma));
 return max(vec3(0.),paper-(paper-c)*(1.+2.5*small*coverage*filtered));
}

// Colors in the design palette are encoded sRGB. Blend glyph coverage and
// filter source mips in linear light; the render target encodes the result once.
fn linear_rgb(c:vec3<f32>)->vec3<f32> {
 return select(pow((c+0.055)/1.055,vec3(2.4)),c/12.92,c<=vec3(0.04045));
}
