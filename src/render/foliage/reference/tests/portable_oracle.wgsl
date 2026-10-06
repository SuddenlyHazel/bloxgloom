// Independent mathematical reference for the audited botanical defaults.
// Evaluated on the test GPU: sine/hash precision varies across GPU vendors,
// so recorded noise outputs from one adapter would not be portable goldens.
var<private> source_time:f32;
var<private> cameraPosition:vec3f;
var<private> relativeEyePosition:vec3f;

fn oracle_hash(cell:vec2f)->f32 {
    return fract(sin(dot(cell,vec2f(12.9898,4.1414)))*43758.5453);
}
fn oracle_field(at:vec2f)->f32 {
    let base=floor(at);
    let fraction=fract(at);
    let blend=fraction*fraction*(vec2f(3.0)-2.0*fraction);
    let left=mix(oracle_hash(base),oracle_hash(base+vec2f(0.0,1.0)),blend.y);
    let right=mix(oracle_hash(base+vec2f(1.0,0.0)),oracle_hash(base+vec2f(1.0,1.0)),blend.y);
    return mix(left,right,blend.x)-0.5;
}
fn WavingBlocks(relative:vec3f,kind:u32,top:f32)->vec3f {
    let world=relative+cameraPosition;
    // density, animation speed, horizontal amplitude, vertical amplitude
    var parameters=vec4f(0.0);
    var bending=false;
    switch kind {
        case 100u: {if top>0.9 {parameters=vec4f(0.35,1.0,0.25,0.06);bending=true;}}
        case 101u: {if top>0.9||fract(world.y+0.005)>0.01 {parameters=vec4f(0.7,1.35,0.12,0.0);bending=true;}}
        case 102u: {if top>0.9||fract(world.y+0.005)>0.01 {parameters=vec4f(0.35,1.15,0.15,0.06);}}
        case 103u: {parameters=vec4f(0.35,1.15,0.15,0.06);}
        case 104u: {if top>0.9||fract(world.y+0.0675)>0.01 {parameters=vec4f(0.35,1.0,0.15,0.06);bending=true;}}
        case 105u: {parameters=vec4f(0.25,1.0,0.08,0.08);}
        case 106u: {parameters=vec4f(0.35,1.25,0.06,0.06);}
        case 107u,157u: {parameters=vec4f(0.5,1.25,0.06,0.0);}
        case 108u: {
            let first=sin(2.0*3.1415927*(source_time*0.7+world.x*0.14+world.z*0.07));
            let second=sin(2.0*3.1415927*(source_time*0.5+world.x*0.10+world.z*0.20));
            return relative+vec3f(0.0,(first+second)*0.0125,0.0);
        }
        default: {return relative;}
    }
    var displacement=vec3f(0.0);
    if parameters.x>0.0 {
        let phase=world*parameters.x+vec3f(source_time*parameters.y);
        let field=vec3f(oracle_field(phase.yz),oracle_field(phase.xz+vec2f(0.333)),oracle_field(phase.xy+vec2f(0.667)));
        displacement=field*vec3f(parameters.zw,parameters.z);
    }
    if bending {
        let toward_eye=relative+relativeEyePosition+vec3f(0.0,0.62,0.0);
        let radius=length(toward_eye*vec3f(4.0,2.0,4.0));
        displacement+=toward_eye*(vec3f(1.0,0.25,1.0)*max(2.0/max(radius,1.0)-0.35,0.0));
    }
    return relative+displacement;
}
