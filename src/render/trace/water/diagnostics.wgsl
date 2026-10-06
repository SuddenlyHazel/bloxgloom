// Linear, additive contribution attribution only. Normal mode0 compiles away
// the selector. Filtering occurs after evaluating the term; path/throughput,
// every random draw and dynamic/visibility query remain unchanged.
fn ray_water_component(value:vec3f,water_scattered:bool)->vec3f {
    if (RAY_WATER_COMPONENT==1u&&water_scattered)||(RAY_WATER_COMPONENT==2u&&!water_scattered) {
        return vec3f(0.0);
    }
    return value;
}
