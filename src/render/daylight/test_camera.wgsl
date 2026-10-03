struct TestLightingCamera { horizon:vec4f, sky_zenith:vec4f, sun_radiance:vec4f, ambient_lower:vec4f, ambient_upper:vec4f };
var<private> camera = TestLightingCamera(vec4f(0.59,0.72,0.82,0.0),vec4f(0.20,0.45,0.75,1.0),vec4f(0.72,0.67,0.56,0.0),vec4f(0.30,0.285,0.26,0.65),vec4f(0.46,0.49,0.54,0.0));
