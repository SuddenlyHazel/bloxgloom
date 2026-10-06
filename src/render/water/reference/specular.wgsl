// Source default sphere-area GGX (smoothness=.9, dielectric F0=.02).
fn bg_water_source_noh(radius:f32,nl:f32,nv:f32,vl:f32)->f32 {
    let rc=inverseSqrt(1.0+radius*radius);let rl=2.0*nl*nv-vl;
    if rl>=rc {return 1.0;}
    let ratio=rc*radius*inverseSqrt(max(1.0-rl*rl,0.00000001));
    var nt=ratio*(nv-rl*nl);var vt=ratio*(2.0*nv*nv-1.0-rl*vl);
    let triple=sqrt(clamp(1.0-nl*nl-nv*nv-vl*vl+2.0*nl*nv*vl,0.0,1.0));
    let nb=ratio*triple;let vb=ratio*(2.0*triple*nv);
    let nlv=nl*rc+nv+nt;let vlv=vl*rc+1.0+vt;
    let p=nb*vlv;let q=nlv*vlv;let s=vb*nlv;
    let numerator=q*(-0.5*p+0.25*vb*nlv);
    let denominator=p*p+s*(s-2.0*p)+nlv*((nl*rc+nv)*vlv*vlv+q*(-0.5*(vlv+vl*rc)-0.5));
    let sum=denominator*denominator+numerator*numerator;
    var twice=0.0;if sum>0.0 {twice=2.0*numerator/sum;}
    let st=twice*denominator;let ct=1.0-twice*numerator;
    nt=ct*nt+st*nb;vt=ct*vt+st*vb;
    let nh=nv+nl*rc+nt;let hh=2.0*(vl*rc+vt)+2.0;
    return clamp(nh*nh/max(hh,0.00000001),0.0,1.0);
}
fn bg_water_source_specular(normal:vec3f,view:vec3f,sky:f32,shadow:f32)->vec3f {
    if sky<=0.0||shadow<=0.0 {return vec3f(0.0);}
    let l=normalize(camera.sun.xyz);let h=normalize(l+view);
    let nl=clamp(dot(normal,l),0.0,1.0);let raw_nv=clamp(dot(normal,view),-1.0,1.0);
    let nv=max(raw_nv,0.0);let vl=dot(l,view);let hl=clamp(dot(h,l),0.0,1.0);
    let radius=0.025*camera.ambient_lower.w+0.05;
    var nh=bg_water_source_noh(radius,nl,raw_nv,vl);
    if raw_nv<0.0 {let c=dot(normal,h);nh=c*c;}
    let roughness=0.01;let roughness2=roughness*roughness;
    let denominator=nh*(roughness2-1.0)+1.0;
    let distribution=roughness2/(3.14159*denominator*denominator);
    let fresnel=exp2((-5.55473*hl-6.98316)*hl)*0.98+0.02;
    let k=roughness*0.5;
    let smith=0.25/((nl*(1.0-k)+k)*(nv*(1.0-k)+k));
    let magnitude=max(sqrt(3.0)*fresnel,0.001);
    let scalar=distribution*magnitude*smith;
    let ggx=vec3f(scalar/(1.0+0.0078125*scalar)*fresnel/magnitude*nl*(1.0-roughness2));
    let fade=1.0-pow(1.0-water_reference.sky.sun_radiance.w,1.5);
    let morning=vec3f(255.0,160.0,80.0)*(1.2/255.0);
    let day=vec3f(196.0,220.0,255.0)*(1.4/255.0);
    let spec_day=mix(sqrt(morning),sqrt(day),fade*0.7);
    let night=vec3f(96.0,192.0,255.0)*(0.3/255.0);
    let spec_night=sqrt(night*water_reference.sky.climate.y*0.2);
    let palette=mix(spec_night,spec_day*spec_day,camera.ambient_lower.w);
    return ggx*palette*palette*sky*shadow*camera.ambient_upper.w*pow(1.0-camera.sun_radiance.w,2.0);
}
