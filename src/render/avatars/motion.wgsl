struct MotionFrame {
    previous: mat4x4f,
    viewport: vec4f,
    depth: vec4f,
};
@group(MOTION_GROUP) @binding(0) var<storage, read> previous_world: array<mat4x4f>;
@group(MOTION_GROUP) @binding(1) var<uniform> motion_frame: MotionFrame;
// RG are signed pixel velocities (half precision keeps subpixel accuracy near
// zero), B is previous linear depth, A is valid (+1) or explicitly reactive (-1).
// Clear A=0 reserves camera reprojection for static surfaces.
fn bg_encode_motion(previous: vec4f, current: vec4f) -> vec4f {
    // Both positions are interpolated from the same current triangle. Comparing
    // against builtin(position) would turn rasterizer subpixel vertex snapping
    // into false motion, especially on thin hair and small GLB geometry.
    let pixel = (current.xy / current.w * vec2f(0.5, -0.5) + vec2f(0.5)) * motion_frame.viewport.xy;
    if previous.w <= 0.0 || motion_frame.depth.z == 0.0 { return vec4f(0.0, 0.0, 0.0, -1.0); }
    let ndc = previous.xyz / previous.w;
    if ndc.z < 0.0 || ndc.z >= 1.0 { return vec4f(0.0, 0.0, 0.0, -1.0); }
    let old_pixel = (ndc.xy * vec2f(0.5, -0.5) + vec2f(0.5)) * motion_frame.viewport.xy + motion_frame.viewport.zw;
    let linear = motion_frame.depth.x / max(1.0 - ndc.z * (1.0 - motion_frame.depth.x / motion_frame.depth.y), 0.000001);
    return vec4f(old_pixel - pixel, linear, 1.0);
}
