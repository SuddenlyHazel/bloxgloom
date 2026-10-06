struct BgUnderwaterFrame {inverse:mat4x4f,eye:vec4f,color:vec4f,options:vec4f};
@group(0) @binding(4) var<uniform> underwater:BgUnderwaterFrame;
fn bg_reference_underwater_uv(top:vec2f)->vec2f {
 let original=vec2f(top.x,1.0-top.y);let time=underwater.options.x;
 let uv=original+vec2f(cos(original.y*32.0+time*3.0),sin(original.x*32.0+time*1.7))*0.0005;
 if any(uv<=vec2f(0.0))||any(uv>=vec2f(1.0)) {return top;}
 return vec2f(uv.x,1.0-uv.y);
}
