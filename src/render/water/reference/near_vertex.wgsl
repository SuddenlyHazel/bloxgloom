@vertex fn vs(v:Input)->Output {
    var world=v.position;
    world.y+=bg_reference_water_wave(world);
    return Output(camera.view_projection*vec4f(world,1.0),world,v.normal,v.color,v.light);
}
