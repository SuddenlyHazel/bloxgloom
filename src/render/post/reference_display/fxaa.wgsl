fn sample_fxaa(uv:vec2f)->vec3f {return sample_color(vec2f(uv.x,1.0-uv.y));}
// Checked-in FXAA311 defaults: sensitivity1, subpixel .5 (.25 with TAA).
fn bg_reference_fxaa(color:vec3f,uv:vec2f)->vec3f {
    let view=1.0/vec2f(textureDimensions(source));
    let center=luma(color);let down=luma(sample_fxaa(uv+vec2f(0.0,-view.y)));
    let up=luma(sample_fxaa(uv+vec2f(0.0,view.y)));let left=luma(sample_fxaa(uv-vec2f(view.x,0.0)));
    let right=luma(sample_fxaa(uv+vec2f(view.x,0.0)));
    let low=min(center,min(min(down,up),min(left,right)));let high=max(center,max(max(down,up),max(left,right)));
    let range=high-low;if range<=max(0.0312,high*0.125) {return color;}
    let dl=luma(sample_fxaa(uv-view));let ur=luma(sample_fxaa(uv+view));
    let ul=luma(sample_fxaa(uv+vec2f(-view.x,view.y)));let dr=luma(sample_fxaa(uv+vec2f(view.x,-view.y)));
    let du=down+up;let lr=left+right;let lc=dl+ul;let rc=dr+ur;
    let horizontal=abs(-2.0*left+lc)+abs(-2.0*center+du)*2.0+abs(-2.0*right+rc);
    let vertical=abs(-2.0*up+ur+ul)+abs(-2.0*center+lr)*2.0+abs(-2.0*down+dl+dr);
    let is_horizontal=horizontal>=vertical;
    let l1=select(left,down,is_horizontal);let l2=select(right,up,is_horizontal);
    let g1=l1-center;let g2=l2-center;let first=abs(g1)>=abs(g2);
    let gradient=0.25*max(abs(g1),abs(g2));
    let step=select(view.x,view.y,is_horizontal)*select(1.0,-1.0,first);
    let average=0.5*(select(l2,l1,first)+center);
    let perpendicular=select(vec2f(step,0.0),vec2f(0.0,step),is_horizontal);
    let along=select(vec2f(0.0,view.y),vec2f(view.x,0.0),is_horizontal);
    var uv1=uv+perpendicular*0.5-along;var uv2=uv+perpendicular*0.5+along;
    var end1=luma(sample_fxaa(uv1))-average;var end2=luma(sample_fxaa(uv2))-average;
    var reached1=abs(end1)>=gradient;var reached2=abs(end2)>=gradient;
    if !reached1 {uv1-=along;}if !reached2 {uv2+=along;}
    let quality=array<f32,12>(1.0,1.0,1.0,1.0,1.0,1.5,2.0,2.0,2.0,2.0,4.0,8.0);
    if !(reached1&&reached2) {
        for(var i=2u;i<12u;i++) {
            if !reached1 {end1=luma(sample_fxaa(uv1))-average;}
            if !reached2 {end2=luma(sample_fxaa(uv2))-average;}
            reached1=abs(end1)>=gradient;reached2=abs(end2)>=gradient;
            if !reached1 {uv1-=along*quality[i];}if !reached2 {uv2+=along*quality[i];}
            if reached1&&reached2 {break;}
        }
    }
    let distance1=select(uv.y-uv1.y,uv.x-uv1.x,is_horizontal);
    let distance2=select(uv2.y-uv.y,uv2.x-uv.x,is_horizontal);
    let first_direction=distance1<distance2;
    let pixel_offset=-min(distance1,distance2)/(distance1+distance2)+0.5;
    let correct=(select(end2,end1,first_direction)<0.0)!=(center<average);
    let local=(2.0*(du+lr)+lc+rc)/12.0;
    let subpixel=clamp(abs(local-center)/range,0.0,1.0);
    let smoothed=(-2.0*subpixel+3.0)*subpixel*subpixel;
    let subpixel_strength=select(0.5,0.25,options.x>0.5);
    let final_offset=max(select(0.0,pixel_offset,correct),smoothed*smoothed*subpixel_strength);
    return sample_fxaa(uv+perpendicular*final_offset);
}
