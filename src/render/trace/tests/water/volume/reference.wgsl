// Appended medium directory shares the loaded-space buffer. Absolute offsets
// are words; the first eight words remain the original authority header.
fn ray_volume_word(index:u32)->u32 {return ray_coverage.mask[index-8u];}
struct RayKnownRegion {state:i32,low:vec3f,high:vec3f};
fn ray_water_region(p:vec3f)->RayKnownRegion {
    let cell=vec3i(floor(p/16.0));let low=vec3f(cell)*16.0;let high=low+vec3f(16.0);
    let header=bitcast<u32>(ray_frame.water.y);
    if ray_loaded_cell(cell) {
        if header==0u {return RayKnownRegion(0,low,high);}
        let size=ray_coverage.dimensions;let r=vec3u(cell-ray_coverage.minimum);
        let index=r.x+size.x*(r.y+size.y*r.z);
        let count=ray_volume_word(header);let directory=ray_volume_word(header+1u);
        var first=0u;var end=count;
        loop {
            if first>=end {break;}
            let middle=first+(end-first)/2u;
            if ray_volume_word(directory+middle*4u)<index {first=middle+1u;}
            else {end=middle;}
        }
        if first>=count {return RayKnownRegion(0,low,high);}
        let entry=directory+first*4u;
        if ray_volume_word(entry)!=index {return RayKnownRegion(0,low,high);}
        let kind=ray_volume_word(entry+1u);
        if kind==1u {return RayKnownRegion(1,low,high);}
        let local=vec3u(vec3i(floor(p))-cell*16);
        let voxel=local.x+16u*(local.z+16u*local.y);
        let bits=ray_volume_word(ray_volume_word(entry+2u)+voxel/32u);
        return RayKnownRegion(select(0,1,(bits&(1u<<(voxel%32u)))!=0u),low,high);
    }
    if header!=0u {
        let count=ray_volume_word(header+2u);let directory=ray_volume_word(header+3u);
        for(var i=0u;i<count;i++) {
            let entry=directory+i*8u;
            let origin=vec2f(f32(bitcast<i32>(ray_volume_word(entry))),f32(bitcast<i32>(ray_volume_word(entry+1u))));
            let width=f32(ray_volume_word(entry+2u));let extent=f32(ray_volume_word(entry+4u));
            if any(p.xz<origin)||any(p.xz>=origin+vec2f(extent)) {continue;}
            let c=vec2u(vec2i(floor(p.xz/width))-vec2i(origin/width));let column=c.x+32u*c.y;
            if column>=ray_volume_word(entry+5u) {continue;}
            let record=ray_volume_word(entry+3u)+column*4u;
            let coverage=ray_volume_word(record);let intervals=ray_volume_word(record+1u);
            for(var j=0u;j<intervals;j++) {
                let bottom=f32(bitcast<i32>(ray_volume_word(coverage+j*2u)));
                let top=f32(bitcast<i32>(ray_volume_word(coverage+j*2u+1u)));
                if p.y<bottom||p.y>=top {continue;}
                let xz=origin+vec2f(c)*width;
                // Clip coarse columns to the16m grid so a wider column cannot
                // skip a nearer loaded chunk that takes precedence.
                let region_low=max(low,vec3f(xz.x,bottom,xz.y));
                let region_high=min(high,vec3f(xz.x+width,top,xz.y+width));
                let wet=ray_volume_word(record+2u);let wet_count=ray_volume_word(record+3u);
                for(var k=0u;k<wet_count;k++) {
                    let a=f32(bitcast<i32>(ray_volume_word(wet+k*2u)));
                    let b=f32(bitcast<i32>(ray_volume_word(wet+k*2u+1u)));
                    if p.y>=a&&p.y<b {return RayKnownRegion(1,region_low,region_high);}
                }
                return RayKnownRegion(0,region_low,region_high);
            }
        }
    }
    return RayKnownRegion(-1,low,high);
}
fn ray_water_at(p:vec3f)->i32 {return ray_water_region(p).state;}
// A fixed epsilon can disappear at ordinary512–1024m world coordinates.
// Advance one representable float along each nonzero axis so exact negative
// crossings select the entered region rather than revisiting the prior cell.
fn ray_volume_entered_point(p:vec3f,direction:vec3f)->vec3f {
    var point=p;
    for(var axis=0u;axis<3u;axis++) {
        if direction[axis]==0.0 {continue;}
        let value=p[axis];var bits=bitcast<u32>(value);
        // Subnormal nextafter(0) can flush or underflow in grid division.
        if value==0.0 {point[axis]=select(-0.0001,0.0001,direction[axis]>0.0);continue;}
        if (direction[axis]>0.0)==(value>0.0) {bits++;}
        else {bits--;}
        point[axis]=bitcast<f32>(bits);
    }
    return point;
}
// Advance exactly to coverage boundaries, including vertical unknown gaps.
// A exhausted traversal returns its proven prefix; it never invents coverage.
fn ray_water_known_distance(origin:vec3f,direction:vec3f,limit:f32)->f32 {
    var distance=0.0;
    var crossing=origin;
    for(var i=0u;i<4096u;i++) {
        let probe=ray_volume_entered_point(crossing,direction);
        let region=ray_water_region(probe);
        if region.state<0 {return distance;}
        let boundary=select(region.low,region.high,direction>=vec3f(0.0));
        let denominator=select(direction,vec3f(0.00000001),abs(direction)<vec3f(0.00000001));
        let exits=(boundary-origin)/denominator;
        let valid=select(vec3f(1e30),exits,abs(direction)>=vec3f(0.00000001));
        let next=min(valid.x,min(valid.y,valid.z));
        if next>=limit {return limit;}
        if next<=distance {return distance;}
        // Reconstruction can round a corner back by several ULPs. Carry
        // exact entered boundaries for every crossed axis, as in grid DDA.
        crossing=select(origin+direction*next,boundary,valid<=vec3f(next));
        distance=next;
    }
    return distance;
}
