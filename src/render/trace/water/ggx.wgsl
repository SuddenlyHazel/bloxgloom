// Proposal-only specular guiding for submerged nonbotanical opaque receivers.
// Evaluation retains the production GGX/Schlick or conductor Smith response.
struct RayWaterGgxEvaluation {response:vec3f,pdf:f32};
struct RayWaterGgxSample {direction:vec3f,weight:vec3f,pdf:f32};
fn ray_water_ggx_evaluate(n:vec3f,v:vec3f,d:vec3f,pbr:BgPbr)->RayWaterGgxEvaluation {
    let h_sum=v+d;let length_squared=dot(h_sum,h_sum);
    if length_squared==0.0 {return RayWaterGgxEvaluation(vec3f(0.0),0.0);}
    let h=h_sum*inverseSqrt(length_squared);
    let nh=max(dot(n,h),0.0);let vh=max(dot(v,h),0.0);
    if nh==0.0||vh==0.0 {return RayWaterGgxEvaluation(vec3f(0.0),0.0);}
    let alpha=pbr.roughness*pbr.roughness;let a2=alpha*alpha;
    let denominator=nh*nh*(a2-1.0)+1.0;
    let distribution=a2/(RAY_PI*denominator*denominator);
    let pdf=distribution*nh/(4.0*vh);
    let nl=max(dot(n,d),0.0);let nv=max(dot(n,v),0.0001);
    let k=(pbr.roughness+1.0)*(pbr.roughness+1.0)/8.0;
    var geometry=nl/max(nl*(1.0-k)+k,0.0001)*nv/max(nv*(1.0-k)+k,0.0001);
    if pbr.preset_id!=0u {geometry=1.0/(1.0+bg_ggx_lambda(nl,alpha)+bg_ggx_lambda(nv,alpha));}
    let response=select(vec3f(0.0),bg_pbr_fresnel(vh,pbr)*distribution*geometry*nh/(4.0*max(nv*nh,0.0001)),nl>0.0);
    return RayWaterGgxEvaluation(response,pdf);
}
fn ray_water_guided_ggx(n:vec3f,v:vec3f,pbr:BgPbr,guide:RayWaterGuide,xi:vec3f)->RayWaterGgxSample {
    let alpha=pbr.roughness*pbr.roughness;
    let cosine=sqrt((1.0-xi.x)/(1.0+(alpha*alpha-1.0)*xi.x));
    let sine=sqrt(max(0.0,1.0-cosine*cosine));let azimuth=2.0*RAY_PI*xi.y;
    let h=ray_basis(n)*vec3f(sine*cos(azimuth),sine*sin(azimuth),cosine);
    var direction=reflect(-v,h);
    if guide.enabled&&xi.z<0.5 {direction=ray_water_cone_direction(guide,xi.xy);}
    let evaluated=ray_water_ggx_evaluate(n,v,direction,pbr);
    var pdf=evaluated.pdf;
    if guide.enabled {pdf=0.5*(pdf+ray_water_cone_pdf(guide,direction));}
    var weight=vec3f(0.0);
    if pdf>0.0 {weight=evaluated.response/pdf;}
    return RayWaterGgxSample(direction,weight,pdf);
}
