@vertex fn vs_main(v:In)->Out {return bg_lod_reference_vertex(v);}
@vertex fn vs_water(v:In)->Out {
    var o=bg_lod_reference_vertex(v);
    let wave=bg_reference_water_wave(o.local+vec3f(tile.origin.xyz));
    o.local.y+=wave;o.relative.y+=wave;
    o.position=camera.view_projection*vec4f(o.relative,1.0);
    return o;
}
