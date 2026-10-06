// Water is a dielectric boundary and a spectral participating volume, never
// an opaque albedo or the raster alpha blend. Equations: PBRT4 dielectric BSDF.
// World distances are metres. Three representative wavelengths (650/550/450nm)
// approximate RGB, not a full spectral reconstruction. Absorption is Pope/Fry
// 1997; molecular scattering uses the 550nm NASA ocean-optics reference and
// wavelength^-4.32 scaling. No source-art alpha is an optical coefficient.
const RAY_WATER_IOR:f32=1.333;
const RAY_WATER_ABSORPTION:vec3f=vec3f(0.340,0.0565,0.00922);
const RAY_WATER_SCATTERING:vec3f=vec3f(0.0006803136,0.0014,0.0033313234);
const RAY_WATER_EXTINCTION:vec3f=RAY_WATER_ABSORPTION+RAY_WATER_SCATTERING;
struct RayWaterInterface {normal:vec3f,outward:vec3f,roughness:f32,eta_from:f32,eta_to:f32};
struct RayWaterSample {direction:vec3f,weight:vec3f,pdf:f32,transmitted:bool,eta:f32};
struct RayWaterEval {f:vec3f,pdf:f32};
struct RayWaterMediumSample {distance:f32,weight:vec3f,event:bool};

fn ray_water_fresnel(cosine:f32,eta_from:f32,eta_to:f32)->f32 {
    let c=clamp(abs(cosine),0.0,1.0);
    let eta=eta_to/eta_from;
    let sine_squared=(1.0-c*c)/(eta*eta);
    if sine_squared>=1.0 {return 1.0;}
    let transmitted=sqrt(max(0.0,1.0-sine_squared));
    let parallel=(eta*c-transmitted)/max(eta*c+transmitted,0.0000001);
    let perpendicular=(c-eta*transmitted)/max(c+eta*transmitted,0.0000001);
    return 0.5*(parallel*parallel+perpendicular*perpendicular);
}

fn ray_water_beer(distance:f32)->vec3f {
    return exp(-RAY_WATER_EXTINCTION*max(distance,0.0));
}

// Scattering-only channel-mixture free flights. Absorption remains analytic
// Beer weighting instead of roulette; collision and survival PDFs use only
// scattering, while the numerator retains the total physical extinction.
// This preserves the same expected RGB transport with lower absorption noise.
fn ray_water_medium_sample(limit:f32,xi:vec2f)->RayWaterMediumSample {
    let channel=min(u32(clamp(xi.x,0.0,0.99999994)*3.0),2u);
    let distance=-log(max(1.0-xi.y,0.00000006))/RAY_WATER_SCATTERING[channel];
    let event=distance<limit;
    let travel=min(distance,limit);
    let transmission=ray_water_beer(travel);
    if event {
        let sampled_survival=exp(-RAY_WATER_SCATTERING*travel);
        let pdf=dot(sampled_survival,RAY_WATER_SCATTERING)/3.0;
        if pdf<=0.0 {return RayWaterMediumSample(travel,vec3f(0.0),true);}
        return RayWaterMediumSample(travel,transmission*RAY_WATER_SCATTERING/pdf,true);
    }
    let survival=dot(exp(-RAY_WATER_SCATTERING*travel),vec3f(1.0/3.0));
    if survival<=0.0 {return RayWaterMediumSample(travel,vec3f(0.0),false);}
    return RayWaterMediumSample(travel,transmission/survival,false);
}

fn ray_water_phase(cosine:f32)->f32 {
    return 3.0*(1.0+cosine*cosine)/(16.0*RAY_PI);
}

// Invert the normalized molecular phase's cubic CDF. Cardano written with
// asinh/sinh avoids cancellation near its zero-angle midpoint.
fn ray_water_phase_direction(incoming:vec3f,xi:vec2f)->vec3f {
    let argument=4.0*clamp(xi.x,0.0,1.0)-2.0;
    let inverse_hyperbolic=log(argument+sqrt(argument*argument+1.0));
    let e=exp(inverse_hyperbolic/3.0);
    let cosine=e-1.0/e;
    let sine=sqrt(max(0.0,1.0-cosine*cosine));
    let azimuth=2.0*RAY_PI*xi.y;
    return ray_basis(incoming)*vec3f(sine*cos(azimuth),sine*sin(azimuth),cosine);
}

fn ray_water_distribution(cosine:f32,alpha:f32)->f32 {
    let a2=alpha*alpha;
    let denominator=cosine*cosine*(a2-1.0)+1.0;
    return a2/(RAY_PI*denominator*denominator);
}

fn ray_water_lambda(cosine:f32,alpha:f32)->f32 {
    let c=max(abs(cosine),0.000001);
    return 0.5*(sqrt(1.0+alpha*alpha*max(0.0,1.0-c*c)/(c*c))-1.0);
}

fn ray_water_visible_normal(normal:vec3f,view:vec3f,alpha:f32,xi:vec2f)->vec3f {
    let basis=ray_basis(normal);
    let local=transpose(basis)*view;
    let visible=normalize(vec3f(alpha*local.xy,max(local.z,0.000001)));
    var tangent=vec3f(1.0,0.0,0.0);
    if visible.z<0.99999 {tangent=normalize(vec3f(-visible.y,visible.x,0.0));}
    let bitangent=cross(visible,tangent);
    let radius=sqrt(clamp(xi.x,0.0,0.99999994));
    let azimuth=2.0*RAY_PI*xi.y;
    let disk_x=radius*cos(azimuth);
    let disk_y=mix(sqrt(max(0.0,1.0-disk_x*disk_x)),radius*sin(azimuth),0.5*(1.0+visible.z));
    let projected=disk_x*tangent+disk_y*bitangent
        +sqrt(max(0.0,1.0-disk_x*disk_x-disk_y*disk_y))*visible;
    return basis*normalize(vec3f(alpha*projected.xy,max(projected.z,0.000001)));
}

// Radiance-mode BSDF and visible-normal PDF for either hemisphere. Geometric
// crossings must agree with the mapped-normal hemisphere; otherwise reject
// the sample instead of leaking illumination through a perturbed boundary.
fn ray_water_eval(boundary:RayWaterInterface,incoming:vec3f,outgoing:vec3f)->RayWaterEval {
    let n=boundary.normal;let v=-incoming;
    let nv=dot(n,v);let nl=dot(n,outgoing);
    let reflection=nl>0.0;
    if nv<=0.0||abs(nl)<=0.000001 {return RayWaterEval(vec3f(0.0),0.0);}
    if (dot(boundary.outward,v)*dot(boundary.outward,outgoing)>0.0)!=reflection {
        return RayWaterEval(vec3f(0.0),0.0);
    }
    let eta=boundary.eta_to/boundary.eta_from;
    let sum=v+outgoing*select(eta,1.0,reflection);
    if dot(sum,sum)<0.000000000001 {return RayWaterEval(vec3f(0.0),0.0);}
    var micro_normal=normalize(sum);if dot(n,micro_normal)<0.0 {micro_normal=-micro_normal;}
    let vh=dot(v,micro_normal);let lh=dot(outgoing,micro_normal);
    if vh<=0.0||((lh>0.0)!=reflection) {return RayWaterEval(vec3f(0.0),0.0);}
    let alpha=max(boundary.roughness*boundary.roughness,0.0001);
    let distribution=ray_water_distribution(max(dot(n,micro_normal),0.0),alpha);
    let lambda_view=ray_water_lambda(nv,alpha);
    let geometry=1.0/(1.0+lambda_view+ray_water_lambda(nl,alpha));
    let visible_pdf=distribution*vh/(nv*(1.0+lambda_view));
    let fresnel=ray_water_fresnel(vh,boundary.eta_from,boundary.eta_to);
    if reflection {
        return RayWaterEval(vec3f(distribution*geometry*fresnel/(4.0*abs(nl)*nv)),
            visible_pdf*fresnel/(4.0*vh));
    }
    let denominator=(lh+vh/eta)*(lh+vh/eta);
    if denominator<=0.000000000001 {return RayWaterEval(vec3f(0.0),0.0);}
    let jacobian=abs(lh)/denominator;
    let value=(1.0-fresnel)*distribution*geometry*abs(lh*vh)/(abs(nl)*nv*denominator*eta*eta);
    return RayWaterEval(vec3f(value),visible_pdf*jacobian*(1.0-fresnel));
}

fn ray_water_micro_normal(boundary:RayWaterInterface,incoming:vec3f,xi:vec2f)->vec3f {
    let alpha=boundary.roughness*boundary.roughness;
    if alpha<0.001 {return boundary.normal;}
    return ray_water_visible_normal(boundary.normal,-incoming,alpha,xi);
}
fn ray_water_sample_micro(boundary:RayWaterInterface,incoming:vec3f,micro_normal:vec3f,transmitted:bool,conditional:bool)->RayWaterSample {
    let n=boundary.normal;let v=-incoming;
    if dot(n,v)<=0.0 {return RayWaterSample(n,vec3f(0.0),0.0,transmitted,1.0);}
    let eta=boundary.eta_to/boundary.eta_from;
    let fresnel=ray_water_fresnel(dot(v,micro_normal),boundary.eta_from,boundary.eta_to);
    let probability=select(fresnel,1.0-fresnel,transmitted);
    if probability<=0.0 {return RayWaterSample(n,vec3f(0.0),0.0,transmitted,1.0);}
    var outgoing=reflect(incoming,micro_normal);
    if transmitted {outgoing=refract(incoming,micro_normal,1.0/eta);}
    let hemisphere=dot(n,outgoing);
    let valid=select(hemisphere>0.0,hemisphere<0.0,transmitted)
        &&((dot(boundary.outward,v)*dot(boundary.outward,outgoing)>0.0)!=transmitted);
    if !valid {return RayWaterSample(n,vec3f(0.0),0.0,transmitted,select(1.0,eta,transmitted));}
    if boundary.roughness*boundary.roughness<0.001 {
        return RayWaterSample(outgoing,vec3f(select(1.0,1.0/(eta*eta),transmitted)*select(1.0,probability,conditional)),
            select(probability,1.0,conditional),transmitted,select(1.0,eta,transmitted));
    }
    let evaluated=ray_water_eval(boundary,incoming,outgoing);
    let pdf=evaluated.pdf/select(1.0,probability,conditional);
    var weight=vec3f(0.0);
    if pdf>0.0 {weight=evaluated.f*abs(hemisphere)/pdf;}
    return RayWaterSample(outgoing,weight,pdf,transmitted,select(1.0,eta,transmitted));
}
fn ray_water_sample(boundary:RayWaterInterface,incoming:vec3f,xi:vec3f)->RayWaterSample {
    let micro_normal=ray_water_micro_normal(boundary,incoming,xi.xy);
    let fresnel=ray_water_fresnel(dot(-incoming,micro_normal),boundary.eta_from,boundary.eta_to);
    return ray_water_sample_micro(boundary,incoming,micro_normal,xi.z>=fresnel,false);
}
// Sample each lobe independently at the first camera interface. Its real
// Fresnel allocation remains in the weight, rather than a binary coin flip.
fn ray_water_conditional_sample(boundary:RayWaterInterface,incoming:vec3f,xi:vec2f,transmitted:bool)->RayWaterSample {
    return ray_water_sample_micro(boundary,incoming,ray_water_micro_normal(boundary,incoming,xi),transmitted,true);
}
fn ray_water_conditional_pdf(boundary:RayWaterInterface,incoming:vec3f,outgoing:vec3f)->f32 {
    let evaluated=ray_water_eval(boundary,incoming,outgoing);
    if evaluated.pdf<=0.0 {return 0.0;}
    let reflected=dot(boundary.normal,outgoing)>0.0;
    let eta=boundary.eta_to/boundary.eta_from;
    var micro_normal=normalize(-incoming+outgoing*select(eta,1.0,reflected));
    if dot(boundary.normal,micro_normal)<0.0 {micro_normal=-micro_normal;}
    let fresnel=ray_water_fresnel(dot(-incoming,micro_normal),boundary.eta_from,boundary.eta_to);
    let probability=select(1.0-fresnel,fresnel,reflected);
    if probability<=0.0 {return 0.0;}
    return evaluated.pdf/probability;
}

// A finite physical solar disc gives BSDF-sampled refracted caustic paths a
// nonzero measure. Its integral preserves the existing directional irradiance;
// no artistic display-disc intensity or arbitrary reflection boost applies.
const RAY_WATER_SUN_RADIUS:f32=0.00465;
fn ray_water_sun_direction(sun:vec3f,xi:vec2f)->vec3f {
    let cosine=mix(1.0,cos(RAY_WATER_SUN_RADIUS),xi.x);
    let sine=sqrt(max(0.0,1.0-cosine*cosine));
    let azimuth=2.0*RAY_PI*xi.y;
    return ray_basis(normalize(sun))*vec3f(sine*cos(azimuth),sine*sin(azimuth),cosine);
}

fn ray_water_sun_cone_pdf()->f32 {return 1.0/(2.0*RAY_PI*(1.0-cos(RAY_WATER_SUN_RADIUS)));}
fn ray_water_sun_pdf(sun:vec3f,direction:vec3f)->f32 {
    if dot(normalize(sun),direction)<cos(RAY_WATER_SUN_RADIUS) {return 0.0;}
    return 1.0/(2.0*RAY_PI*(1.0-cos(RAY_WATER_SUN_RADIUS)));
}

fn ray_water_sun_emission(sun:vec3f,solar:vec3f,direction:vec3f)->vec3f {
    if ray_water_sun_pdf(sun,direction)<=0.0 {return vec3f(0.0);}
    let sine=sin(RAY_WATER_SUN_RADIUS);
    return solar/(RAY_PI*sine*sine);
}

fn ray_water_power_weight(a:f32,b:f32)->f32 {
    if a<=0.0 {return 0.0;}
    let ratio=b/a;
    return 1.0/(1.0+ratio*ratio);
}

fn ray_water_is(hit:RayHit)->bool {
    if hit.triangle==0xffffffffu||(hit.triangle&0x80000000u)!=0u {return false;}
    return (ray_triangle_at(hit.triangle).surface_flags&2u)!=0u;
}

fn ray_water_interface(hit:RayHit,incoming:vec3f,position:vec3f,footprint:vec4f)->RayWaterInterface {
    let outward=normalize(ray_triangle_at(hit.triangle).normal.xyz);
    let entering=dot(incoming,outward)<0.0;
    var normal=outward;var roughness=0.12;
    if abs(normal.y)>0.5 {
        let waves=bg_water_waves(position,ray_frame.water.x,footprint);
        normal=normalize(normal+vec3f(waves.x,0.0,waves.y));
        roughness=pow(pow(roughness,4.0)+waves.z,0.25);
    }
    normal=select(-normal,normal,entering);
    if dot(normal,-incoming)<=0.0 {normal=select(-outward,outward,entering);}
    return RayWaterInterface(normal,outward,roughness,
        select(RAY_WATER_IOR,1.0,entering),select(1.0,RAY_WATER_IOR,entering));
}
