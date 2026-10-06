struct TestLightingCamera { horizon:vec4f, sky_zenith:vec4f, sun_radiance:vec4f, ambient_lower:vec4f, ambient_upper:vec4f,ambient_sh:array<vec4f,6> };
var<private> camera = TestLightingCamera(vec4f(0.59,0.72,0.82,0.0),vec4f(0.20,0.45,0.75,1.0),vec4f(1.728,1.608,1.344,0.0),vec4f(0.36,0.335,0.30,0.65),vec4f(0.55,0.57,0.60,0.0),array<vec4f,6>());
