// Shared builtin player surface artwork/customization. Lighting is separate.
fn bg_actor_primitive_color(part:u32,cosmetics:vec3u,base:vec3f)->vec3f {
    if part==1u {return SHIRTS[min(cosmetics.y,31u)];}
    if part==2u {return PANTS[min(cosmetics.z,31u)];}
    if part==3u {return mix(PANTS[min(cosmetics.z,31u)],vec3f(0.10,0.08,0.07),0.65);}
    if part==4u {return vec3f(0.025,0.035,0.045);}
    if part>=5u {return base;}
    return SKINS[min(cosmetics.x,31u)];
}
fn bg_actor_hair_color(neutral:vec4f,rgb:vec3f,fixed:bool)->vec4f {
    return vec4f(select(neutral.rgb*bg_actor_srgb(rgb/255.0),neutral.rgb,fixed),neutral.a);
}
fn bg_actor_body_color(sample:vec4f,surface:u32,cosmetics:vec3u,iris:vec4u)->vec4f {
    var color=sample;
    if surface==0u&&cosmetics.x!=0u {
        color=vec4f(clamp(dot(color.rgb,vec3f(0.2126,0.7152,0.0722))/0.425405,0.0,1.0)*SKINS[min(cosmetics.x,31u)],color.a);
    }
    if surface==1u&&cosmetics.z!=0u {
        color=vec4f(clamp(dot(color.rgb,vec3f(0.2126,0.7152,0.0722))/0.08,0.0,1.0)*PANTS[min(cosmetics.z,31u)],color.a);
    }
    if surface==9u&&cosmetics.y!=0u {
        color=vec4f(clamp(dot(color.rgb,vec3f(0.2126,0.7152,0.0722))/0.08,0.0,1.0)*SHIRTS[min(cosmetics.y,31u)],color.a);
    }
    if surface==4u&&iris.w!=0u {color=vec4f(bg_actor_srgb(vec3f(iris.xyz)/255.0),color.a);}
    return color;
}
