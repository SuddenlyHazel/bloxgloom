// Shared raster/ray deformation math. No animation clock or gameplay authority.
fn bg_actor_local(p:vec3f,part:u32,pose:vec4f)->vec3f {
    var local=p;
    if part==10u {local.y+=max(0.0,pose.y)*0.045;}
    if part==11u {local.y+=max(0.0,-pose.y)*0.045;}
    if part>=5u&&part!=12u {
        local.y*=1.0-pose.w;local.x*=1.0+pose.w*0.5;local.z*=1.0+pose.w*0.5;
        if part!=10u&&part!=11u {local.y+=pose.z;}
        if p.y>0.65 {local.z+=pose.z*2.0;}
    }
    return local;
}
fn bg_actor_quaternion(p:vec3f,orientation:vec4f)->vec3f {
    let q=normalize(orientation);return p+2.0*cross(q.xyz,cross(q.xyz,p)+q.w*p);
}
fn bg_actor_normal(n:vec3f,transform:mat4x4f)->vec3f {
    let a=transform[0].xyz;let b=transform[1].xyz;let c=transform[2].xyz;
    let determinant=dot(a,cross(b,c));
    let cofactor=cross(b,c)*n.x+cross(c,a)*n.y+cross(a,b)*n.z;
    if dot(cofactor,cofactor)>0.000000000001 {
        return normalize(cofactor*select(1.0,-1.0,determinant<0.0));
    }
    return n;
}
fn bg_actor_srgb(rgb:vec3f)->vec3f {
    return select(pow((rgb+vec3f(0.055))/1.055,vec3f(2.4)),rgb/12.92,rgb<=vec3f(0.04045));
}
