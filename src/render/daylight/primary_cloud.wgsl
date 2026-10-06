// The volume's world-space density owns primary solar attenuation whenever
// scene transport is active. Ambient and local light keep independent energy.
fn bg_primary_sun_transmittance(world:vec3f)->f32 {
    if camera.cloud.w<=0.5 {return 1.0;}
    return bg_cloud_transmittance(world,normalize(camera.sun.xyz),camera.cloud.x,camera.cloud.yz);
}
