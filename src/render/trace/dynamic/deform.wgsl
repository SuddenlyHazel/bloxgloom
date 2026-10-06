@group(0) @binding(3) var<uniform> dyn_step:vec4u;
struct DynVertex {position:vec3f,normal:vec3f,uv:vec2f};
fn dyn_deform_vertex(vertex:u32,instance:u32)->DynVertex {
    let p=dyn_av3(vertex);let n=dyn_av3(vertex+4u);let uv=vec2f(dyn_af(vertex+8u),dyn_af(vertex+9u));
    let world=dyn_matrix(instance+8u);let mode=dyn_frame[instance+7u];
    var local=p;var normal=n;
    if mode==0u {
        let part=dyn_assets[vertex+3u];
        local=bg_actor_local(p,part,dyn_fv4(instance+24u));
        if part==12u {local=bg_actor_quaternion(local,dyn_fv4(instance+36u));
            normal=bg_actor_quaternion(normal,dyn_fv4(instance+36u));}
    } else if mode==1u {
        let transform=dyn_matrix(dyn_frame[instance+5u]+dyn_assets[vertex+10u]*16u);
        local=(transform*vec4f(p,1.0)).xyz;normal=normalize((transform*vec4f(n,0.0)).xyz);
    } else if mode==2u {
        let joints=dyn_frame[instance+5u];
        let transform=dyn_matrix(joints+dyn_assets[vertex+10u]*16u)*dyn_af(vertex+14u)
            +dyn_matrix(joints+dyn_assets[vertex+11u]*16u)*dyn_af(vertex+15u)
            +dyn_matrix(joints+dyn_assets[vertex+12u]*16u)*dyn_af(vertex+16u)
            +dyn_matrix(joints+dyn_assets[vertex+13u]*16u)*dyn_af(vertex+17u);
        local=(transform*vec4f(p,1.0)).xyz;normal=bg_actor_normal(n,transform);
    }
    return DynVertex((world*vec4f(local,1.0)).xyz,normalize((world*vec4f(normal,0.0)).xyz),uv);
}
fn dyn_write4(at:u32,v:vec4f) {
    dyn_geometry[at]=bitcast<u32>(v.x);dyn_geometry[at+1u]=bitcast<u32>(v.y);
    dyn_geometry[at+2u]=bitcast<u32>(v.z);dyn_geometry[at+3u]=bitcast<u32>(v.w);
}
@compute @workgroup_size(64) fn dynamic_deform(@builtin(global_invocation_id) id:vec3u) {
    let index=id.x;if index>=dyn_frame[1] {return;}
    let instance=dyn_instance_for_triangle(index);let asset=dyn_frame[instance];
    let input=dyn_assets[asset+1u]+(index-dyn_frame[instance+1u])*8u;
    let vertices=dyn_assets[asset];let material=dyn_assets[asset+2u]+dyn_assets[input+3u]*16u;
    let part=dyn_assets[input+4u];var visible=1u;
    let mode=dyn_frame[instance+7u];
    if mode==1u {
        let group=dyn_assets[material+13u];let recipe=dyn_frame[instance+33u];
        let body=select(0u,14u,(recipe>>24u)!=0u);let hair=(recipe>>16u)&255u;
        if group!=body&&!(hair>0u&&group==hair) {visible=0u;}
    } else if mode==2u {
        if dyn_frame[dyn_frame[instance+6u]+part*8u+4u]==0u {visible=0u;}
    }
    let a=dyn_deform_vertex(vertices+dyn_assets[input]*20u,instance);
    let b=dyn_deform_vertex(vertices+dyn_assets[input+1u]*20u,instance);
    let c=dyn_deform_vertex(vertices+dyn_assets[input+2u]*20u,instance);
    let out=8u+index*DYN_TRIANGLE_WORDS;
    dyn_write4(out,vec4f(a.position,0.0));dyn_write4(out+4u,vec4f(b.position,0.0));dyn_write4(out+8u,vec4f(c.position,0.0));
    dyn_write4(out+12u,vec4f(a.normal,0.0));dyn_write4(out+16u,vec4f(b.normal,0.0));dyn_write4(out+20u,vec4f(c.normal,0.0));
    dyn_write4(out+24u,vec4f(a.uv,b.uv));dyn_write4(out+28u,vec4f(c.uv,0.0,0.0));
    dyn_geometry[out+32u]=material;dyn_geometry[out+33u]=instance;dyn_geometry[out+34u]=part;dyn_geometry[out+35u]=visible;
}
@compute @workgroup_size(64) fn dynamic_refit(@builtin(global_invocation_id) id:vec3u) {
    let index=id.x;if index>=dyn_frame[2] {return;}
    let at=dyn_frame[4]+index*DYN_NODE_WORDS;
    if dyn_geometry[at+10u]!=dyn_step.x {return;}
    var low=vec3f(3.402823e38);var high=-low;
    let count=dyn_geometry[at+7u];let first=dyn_geometry[at+3u];
    if count==0u {
        let a=dyn_frame[4]+(index+1u)*DYN_NODE_WORDS;
        let b=dyn_frame[4]+dyn_geometry[at+9u]*DYN_NODE_WORDS;
        low=min(dyn_gv3(a),dyn_gv3(b));high=max(dyn_gv3(a+4u),dyn_gv3(b+4u));
    } else if dyn_geometry[at+11u]!=0u {
        let instance=8u+first*DYN_INSTANCE_WORDS;
        let root=dyn_frame[4]+dyn_frame[instance+2u]*DYN_NODE_WORDS;
        if dyn_frame[instance+4u]!=0u {low=dyn_gv3(root);high=dyn_gv3(root+4u);}
    } else {
        for(var i=first;i<first+count;i++) {
            let t=8u+i*DYN_TRIANGLE_WORDS;
            if dyn_geometry[t+35u]==0u {continue;}
            low=min(low,min(dyn_gv3(t),min(dyn_gv3(t+4u),dyn_gv3(t+8u))));
            high=max(high,max(dyn_gv3(t),max(dyn_gv3(t+4u),dyn_gv3(t+8u))));
        }
        low-=vec3f(0.0001);high+=vec3f(0.0001);
    }
    dyn_geometry[at]=bitcast<u32>(low.x);dyn_geometry[at+1u]=bitcast<u32>(low.y);dyn_geometry[at+2u]=bitcast<u32>(low.z);
    dyn_geometry[at+4u]=bitcast<u32>(high.x);dyn_geometry[at+5u]=bitcast<u32>(high.y);dyn_geometry[at+6u]=bitcast<u32>(high.z);
}
