// Loaded 3D cells, including empty chunks. No procedural geometry is authority.
struct RayCoverage { minimum:vec3i, exterior:i32, dimensions:vec3u, cells:u32, mask:array<u32> };
@group(0) @binding(10) var<storage,read> ray_coverage:RayCoverage;
fn ray_loaded_cell(cell:vec3i)->bool {
    let relative=cell-ray_coverage.minimum;
    if ray_coverage.cells==0u||any(relative<vec3i(0))||any(vec3u(relative)>=ray_coverage.dimensions) {return false;}
    let p=vec3u(relative);let size=ray_coverage.dimensions;
    let index=p.x+size.x*(p.y+size.y*p.z);
    return (ray_coverage.mask[index/32u]&(1u<<(index%32u)))!=0u;
}
// A miss through a side or unloaded roof is not proof of sky. Only a positive
// path through loaded cells to the upper exterior boundary upgrades zero sky.
// Saved edits outside client interest remain unknown; this certifies the
// loaded enclosure and procedural ceiling, not an infinite authoritative world.
fn ray_sky_exit_distance(origin:vec3f,direction:vec3f)->f32 {
    if direction.y<=0.00001||ray_coverage.cells==0u {return -1.0;}
    let exterior=f32(ray_coverage.exterior)-0.12;
    let distance=max(exterior-origin.y,0.0)/direction.y;
    if distance>512.0 {return -1.0;}
    var cell=vec3i(floor(origin/16.0));
    let step=vec3i(select(vec3f(-1.0),vec3f(1.0),direction>=vec3f(0.0)));
    let inverse=1.0/max(abs(direction),vec3f(0.00000001));
    let boundary=vec3f(cell+select(vec3i(0),vec3i(1),step>vec3i(0)))*16.0;
    var next=abs(boundary-origin)*inverse;
    let delta=16.0*inverse;
    for(var iteration=0u;iteration<128u;iteration++) {
        if !ray_loaded_cell(cell) {return -1.0;}
        let advance=min(next.x,min(next.y,next.z));
        if advance>=distance {return distance;}
        // Advance every tied axis so exact corner crossings neither skip an
        // unknown cell entered for positive length nor loop on zero-distance edges.
        let axes=next<=vec3f(advance);
        cell+=select(vec3i(0),step,axes);
        next+=select(vec3f(0.0),delta,axes);
    }
    return -1.0;
}

fn ray_certified_sky(origin:vec3f,direction:vec3f)->bool {
    return ray_sky_exit_distance(origin,direction)>=0.0;
}
