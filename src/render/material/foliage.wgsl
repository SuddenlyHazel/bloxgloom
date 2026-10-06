// Presentation-only world-space wind. Material metadata opts in builtin
// botanical cutouts; industrial cutouts and package vertex hooks stay still.
// This same function is used before camera and sun/local-light projection.
fn bg_foliage_wind(position: vec3f, normal: vec3f, uv: vec2f, seconds: f32) -> vec3f {
    let phase = position.x * 0.48 + position.z * 0.31;
    let time = seconds * (6.28318530718 / 128.0);
    let gust = sin(time * 17.0 + phase) * 0.72
        + sin(time * 29.0 + phase * 1.37) * 0.28;
    // Plants use upward diffuse normals. Their roots remain anchored; leaf
    // cards have tilted plane normals and sway as a connected canopy surface.
    let plant = normal.y > 0.9999;
    let weight = select(1.0, pow(clamp(1.0-uv.y,0.0,1.0),2.0), plant);
    let amplitude = select(0.035,0.11,plant) * weight;
    return position + vec3f(gust,0.12*gust,sin(time*17.0+phase+1.2)*0.65)*amplitude;
}
