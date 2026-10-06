//! Asserted test-string hooks; never edit the production shader to profile it.
const COUNTERS: &str = r#"
var<private> probe_nodes:u32;var<private> probe_triangles:u32;var<private> probe_known:u32;var<private> probe_cloud:u32;var<private> probe_vertices:u32;
var<private> probe_regions:u32;var<private> probe_directory:u32;var<private> probe_density:u32;var<private> probe_material:u32;var<private> probe_alpha_tests:u32;
var<private> probe_primary_lighting:u32;var<private> probe_primary_medium:u32;var<private> probe_primary_split:u32;var<private> probe_guides:u32;var<private> probe_replays:u32;
fn probe_alpha_value(uv:vec2f,layer:i32)->f32 {
 probe_alpha_tests++;return textureSampleLevel(ray_albedo,ray_sampler,uv,layer,0.0).a;
}
"#;

fn hook(source: &mut String, anchor: &str, count: usize, increment: &str) {
    assert_eq!(
        source.matches(anchor).count(),
        count,
        "production probe anchor changed: {anchor}"
    );
    *source = source.replace(anchor, &format!("{anchor}{increment}"));
}

pub(in crate::render::trace) fn instrument(mut source: String, instrumented: bool) -> String {
    source = format!("{COUNTERS}\n{source}");
    if instrumented {
        hook(
            &mut source,
            "let node=ray_nodes[index];",
            3,
            "probe_nodes++;",
        );
        hook(
            &mut source,
            "let node=ray_lod_node(page,index);",
            2,
            "probe_nodes++;",
        );
        hook(
            &mut source,
            "let at=dyn_geometry[4]+node*DYN_NODE_WORDS;",
            1,
            "probe_nodes++;",
        );
        hook(
            &mut source,
            "let leaf=dyn_geometry[4]+blas*DYN_NODE_WORDS;",
            1,
            "probe_nodes++;",
        );
        hook(
            &mut source,
            "fn ray_test_triangle(origin:vec3f,direction:vec3f,limit:f32,i:u32,current:RayHit,t:RayTriangle)->RayHit {",
            1,
            "probe_triangles++;",
        );
        hook(
            &mut source,
            "fn ray_lod_candidate(origin:vec3f,direction:vec3f,limit:f32,i:u32,current:RayHit)->RayHit {",
            1,
            "probe_triangles++;",
        );
        // The near any-opaque fast path tests triangles inline, not through
        // ray_test_triangle. LOD's equivalent already uses a counted helper.
        hook(
            &mut source,
            "if (metadata.flags&64u)!=0u || (t.surface_flags&2u)!=0u {continue;}",
            1,
            "probe_triangles++;",
        );
        hook(
            &mut source,
            "fn dyn_triangle(origin:vec3f,direction:vec3f,limit:f32,index:u32,current:RayHit)->RayHit {",
            1,
            "probe_triangles++;",
        );
        hook(
            &mut source,
            "for(var i=0u;i<4096u;i++) {",
            1,
            "probe_known++;",
        );
        hook(
            &mut source,
            "for(var iteration=0u;iteration<128u;iteration++) {",
            1,
            "probe_known++;",
        );
        hook(
            &mut source,
            "for(var i=0u;i<256u;i++) {",
            1,
            "probe_cloud++;",
        );
        // Count only entered vertices, after the shared total-depth guard.
        hook(
            &mut source,
            "if bounce+initial_depth>=12u {break;}",
            1,
            "probe_vertices++;",
        );
        hook(
            &mut source,
            "fn ray_volume_region(p:vec3f,classify_water:bool,indexed:bool)->RayKnownRegion {",
            1,
            "probe_regions++;",
        );
        for (anchor, count) in [
            ("let middle=first+(end-first)/2u;", 1),
            (
                "for(var candidate=0u;candidate<candidates;candidate++) {",
                1,
            ),
            ("for(var j=0u;j<intervals;j++) {", 1),
            ("for(var k=0u;k<wet_count;k++) {", 1),
        ] {
            hook(&mut source, anchor, count, "probe_directory++;");
        }
        hook(
            &mut source,
            "fn bg_cloud_density(world:vec3f,coverage:f32,drift:vec2f)->f32 {",
            1,
            "probe_density++;",
        );
        hook(
            &mut source,
            "fn ray_surface(hit:RayHit,position:vec3f)->RaySurface {",
            1,
            "probe_material++;",
        );
        hook(
            &mut source,
            "fn dyn_alpha(triangle:u32,uv:vec2f)->f32 {",
            1,
            "probe_alpha_tests++;",
        );
        hook(
            &mut source,
            "position:vec3f,raster_indirect:vec4f)->RayLightingSample {",
            1,
            "probe_primary_lighting++;",
        );
        hook(
            &mut source,
            "fn ray_primary_medium_sample(origin:vec3f,direction:vec3f,distance:f32,color:vec3f,correction:vec3f)->RayPrimaryMedium {",
            1,
            "probe_primary_medium++;",
        );
        hook(
            &mut source,
            "fn ray_primary_water_radiance(hit:RayHit,position:vec3f,incoming:vec3f,footprint:vec4f)->vec3f {",
            1,
            "probe_primary_split++;",
        );
        hook(
            &mut source,
            "fn ray_primary_water_lobes(hit:RayHit,position:vec3f,incoming:vec3f,footprint:vec4f)->RayWaterLobes {",
            1,
            "probe_primary_split++;",
        );
        hook(
            &mut source,
            "fn ray_water_guide_hit(origin:vec3f,axis:vec3f)->RayHit {",
            1,
            "probe_guides++;",
        );
        hook(
            &mut source,
            "if ray_dynamic_touched&&!primary_actor {",
            1,
            "probe_replays++;",
        );
        for (original, replacement) in [
            (
                "&& textureSampleLevel(ray_albedo,ray_sampler,uv,i32(ray_materials[u32(t.a.w)].layer),0.0).a<0.5",
                "&& probe_alpha_value(uv,i32(ray_materials[u32(t.a.w)].layer))<0.5",
            ),
            (
                "&& textureSampleLevel(ray_albedo,ray_sampler,uv,i32(metadata.layer),0.0).a<0.5",
                "&& probe_alpha_value(uv,i32(metadata.layer))<0.5",
            ),
            (
                "if textureSampleLevel(ray_albedo,ray_sampler,uv,i32(ray_materials[material].layer),0.0).a<0.5",
                "if probe_alpha_value(uv,i32(ray_materials[material].layer))<0.5",
            ),
        ] {
            assert_eq!(
                source.matches(original).count(),
                1,
                "production alpha hook changed"
            );
            source = source.replace(original, replacement);
        }
    }
    source
}
