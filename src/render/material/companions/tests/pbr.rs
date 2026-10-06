//! Read back production packed-material decoding and energy/darkness boundaries.
use wgpu::util::DeviceExt;
const CASES: u32 = 40;
const FIXTURE: &str = r#"
@group(0) @binding(0) var<storage, read_write> result: array<vec4f>;
@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id: vec3u) {
    let albedo = vec3f(0.5);
    if id.x>=33u {
        let albedo=select(vec3f(0.01,0.03,0.05),vec3f(0.9,0.7,0.4),id.x==34u);
        let green=select(230u,240u,id.x==38u);
        let p=bg_decode_bsl_advanced(vec4f(0.8,f32(green)/255.0,0.0,0.0),albedo,true,true);
        if id.x<=34u {result[id.x]=vec4f(p.f0,select(0.0,1.0,p.artistic));}
        else if id.x==35u {result[id.x]=vec4f(bg_pbr_fresnel(1.0,p),1.0);}
        else if id.x==36u {result[id.x]=vec4f(bg_pbr_fresnel(0.5,p),1.0);}
        else if id.x==37u||id.x==38u {result[id.x]=vec4f(bg_pbr_fresnel(0.0,p),1.0);}
        else {result[id.x]=vec4f(bg_pbr_material_environment_weight(0.0,p),1.0);}
    } else if id.x < 6u {
        let greens = array<f32,6>(10.0,229.0,230.0,231.0,234.0,255.0);
        let p = bg_decode_pbr(vec4f(0.8,greens[id.x]/255.0,0.0,0.0),albedo,true,true);
        result[id.x] = vec4f(p.f0,p.metal);
    } else if id.x < 9u {
        let blues = array<f32,3>(65.0,255.0,64.0);
        let p = bg_decode_pbr(vec4f(0.8,10.0/255.0,blues[id.x-6u]/255.0,0.0),albedo,true,true);
        result[id.x] = vec4f(p.subsurface,p.porosity,p.emission,1.0);
    } else if id.x < 11u {
        let alpha = select(1.0,254.0/255.0,id.x==10u);
        let p = bg_decode_pbr(vec4f(0.8,10.0/255.0,0.0,alpha),albedo,true,true);
        result[id.x] = vec4f(p.emission,0.0,0.0,1.0);
    } else if id.x < 13u {
        result[id.x] = bg_normal_data(vec4f(128.0/255.0,128.0/255.0,32.0/255.0,1.0),id.x==11u);
    } else if id.x>=24u {
        let presets=array<u32,4>(230u,231u,232u,234u);
        var preset=230u;
        if id.x<28u {preset=presets[id.x-24u];}
        var roughness=0.2;
        if id.x==31u {roughness=0.7;}
        if id.x==32u {roughness=0.8;}
        let p=bg_decode_pbr(vec4f(1.0-roughness,f32(preset)/255.0,0.0,0.0),vec3f(0.2,0.4,0.8),true,true);
        if id.x<29u {result[id.x]=vec4f(bg_pbr_fresnel(select(0.5,0.1,id.x==28u),p),f32(p.preset_id));}
        else {
            var cosine=0.0;
            if id.x==29u {cosine=1.0;}
            if id.x==31u {cosine=0.4;}
            result[id.x]=vec4f(bg_pbr_material_environment_weight(cosine,p),f32(p.preset_id));
        }
    } else if id.x>=20u {
        if id.x==20u {let p=bg_bsl_default_material();result[id.x]=vec4f(p.f0,select(0.0,1.0,p.present));}
        else if id.x==21u {result[id.x]=vec4f(bg_bsl_advanced_preset_base(230u),1.0);}
        else if id.x==22u {result[id.x]=vec4f(bg_bsl_advanced_complex_fresnel(1.0,230u),1.0);}
        else {result[id.x]=vec4f(bg_bsl_advanced_complex_fresnel(0.0,230u),1.0);}
    } else if id.x >= 16u {
        let p=bg_decode_pbr(vec4f(0.8,select(230.0,255.0,id.x==18u)/255.0,0.0,0.0),vec3f(0.01,0.03,0.05),true,true);
        if id.x==19u {result[id.x]=vec4f(bg_pbr_material_environment_weight(0.0,p),1.0);}
        else {result[id.x]=vec4f(bg_pbr_fresnel(select(0.0,1.0,id.x==16u),p),1.0);}
    } else {
        let absent = id.x==13u;
        let metal = id.x==14u;
        let p = bg_decode_pbr(vec4f(0.8,select(10.0,231.0,metal)/255.0,0.0,0.0),albedo,true,!absent);
        let up = vec3f(0.0,1.0,0.0);
        if id.x==15u {
            let reflected = bg_pbr_environment(up,up,p,vec3f(1.0),1.0,vec3f(0.0),0.0);
            result[id.x] = vec4f(reflected+vec3f(bg_pbr_diffuse_weight(p,1.0)),1.0);
        } else {
            let reflected = bg_pbr_environment(up,up,p,vec3f(1.0),0.0,vec3f(1.0),0.0)
                +bg_pbr_sun(up,up,vec4f(up,1.0),0.0,1.0,p,vec3f(1.0));
            result[id.x] = vec4f(reflected,bg_pbr_diffuse_weight(p,1.0));
        }
    }
}
"#;
fn source() -> String {
    format!("{}\n{FIXTURE}", include_str!("../../pbr.wgsl"))
}
#[test]
fn packed_material_shader_validates_without_a_gpu() {
    let module = wgpu::naga::front::wgsl::parse_str(&source()).unwrap();
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
}
#[test]
fn gpu_lab_pbr_decodes_f0_metals_sss_ao_emission_and_preserves_darkness() {
    let instance = wgpu::Instance::default();
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&Default::default())) else {
        eprintln!("packed material GPU test skipped: no adapter");
        return;
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("packed material decoding regression"),
        source: wgpu::ShaderSource::Wgsl(source().into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    let output = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &[0; CASES as usize * 16],
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(CASES * 16),
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: output.as_entire_binding(),
        }],
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(CASES, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, u64::from(CASES * 16));
    queue.submit([encoder.finish()]);
    let slice = readback.slice(..);
    slice.map_async(wgpu::MapMode::Read, |result| result.unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let bytes = slice.get_mapped_range().unwrap();
    let rows: &[[f32; 4]] = bytemuck::cast_slice(&bytes);
    assert!(rows.iter().flatten().all(|v| v.is_finite()));
    for (id, expected) in [(0, 10.0 / 255.0), (1, 229.0 / 255.0)] {
        assert!(rows[id][..3].iter().all(|v| (v - expected).abs() < 1e-6));
        assert_eq!(
            rows[id][3], 0.0,
            "dielectric green is reflectance, not metalness"
        );
    }
    assert!(rows[2][0] > 0.2 && rows[2][0] < 0.4);
    assert!(
        rows[3][0] > rows[3][1] && rows[3][1] > rows[3][2],
        "gold conductor tint"
    );
    assert!(
        rows[4][0] > rows[4][1] && rows[4][1] > rows[4][2],
        "copper conductor tint"
    );
    assert!(rows[2..6].iter().all(|row| row[3] == 1.0));
    assert_eq!(rows[5], [0.5, 0.5, 0.5, 1.0]);
    assert!(rows[6][..3].iter().all(|v| v.abs() < 1e-6));
    assert_eq!(rows[6][3], 1.0);
    assert_eq!(rows[7], [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(rows[8], [0.0, 1.0, 0.0, 1.0]);
    assert_eq!(rows[9][0], 0.0, "255 means no emission");
    assert!((rows[10][0] - 1.0).abs() < 1e-6);
    assert!(
        rows[11][2] > 0.99,
        "AO channel does not invert surface normal"
    );
    assert!((rows[11][3] - 32.0 / 255.0).abs() < 1e-6);
    assert!(rows[12][2] < -0.99, "legacy RGB normal contract preserved");
    assert_eq!(rows[12][3], 1.0);
    assert_eq!(
        rows[13],
        [0.0, 0.0, 0.0, 1.0],
        "missing maps preserve legacy diffuse"
    );
    assert_eq!(
        rows[14], [0.0; 4],
        "sealed darkness has no reflections or metal diffuse"
    );
    assert!(
        rows[15][..3].iter().all(|v| *v >= 0.0 && *v <= 1.0),
        "bounded reflected plus diffuse energy"
    );
    assert_eq!(
        rows[20], [0.0; 4],
        "checked-in BSL default disables packed PBR"
    );
    for (channel, value) in [0.24867, 0.22965, 0.21366].into_iter().enumerate() {
        assert!(
            (rows[21][channel] - value).abs() < 1e-6,
            "BSL GetMetalCol oracle"
        );
        let measured = rows[2][channel] * 2.0;
        assert!(
            (rows[22][channel] - measured * measured).abs() < 1e-6,
            "BSL squares measured complex Fresnel"
        );
        assert!(
            (rows[23][channel] - 1.0).abs() < 1e-6,
            "BSL advanced ALBEDO_METAL off leaves grazing white"
        );
    }
    assert_eq!(
        rows[33], rows[34],
        "runtime BSL preset base is independent of albedo"
    );
    for (channel, value) in [0.24867, 0.22965, 0.21366].into_iter().enumerate() {
        assert!((rows[33][channel] - value).abs() < 1e-6);
        assert!(
            (rows[35][channel] - rows[22][channel]).abs() < 1e-6,
            "runtime source complex Fresnel, not enhanced conductor"
        );
    }
    // Independent GLSL rational amplitudes at cosine .5, then squared.
    for (channel, value) in [0.266_906_3, 0.249_570_2, 0.23906528]
        .into_iter()
        .enumerate()
    {
        assert!(
            (rows[36][channel] - value).abs() < 2e-6,
            "artistic angle {:?}",
            rows[36]
        );
    }
    assert_eq!(rows[37], [1.0; 4], "ALBEDO_METAL off gives white grazing");
    assert_eq!(
        rows[38], [1.0; 4],
        "source reserved metal indices use untinted fallback"
    );
    assert!(
        rows[39][..3].iter().all(|v| *v > 0.2 && *v <= 1.0),
        "artistic environment reflection does not retain dark albedo tint"
    );
    // Independently evaluated complex Snell amplitudes (not the real-form
    // implementation): eta=n+ik, transmitted cosine=sqrt(1-sin²/eta²), then
    // half the sum of squared s/p complex amplitudes. Albedo=.2/.4/.8.
    let angular_goldens = [
        [0.1028659, 0.19896063, 0.3890091],
        [0.18773979, 0.31211692, 0.33038184],
        [0.17922725, 0.3595246, 0.72549045],
        [0.1836305, 0.28785503, 0.41419128],
        [0.12668009, 0.25090274, 0.51854247],
    ];
    for (row, expected) in rows[24..29].iter().zip(angular_goldens) {
        for (actual, expected) in row[..3].iter().zip(expected) {
            assert!(
                (actual - expected).abs() < 2e-6,
                "exact conductor angular Fresnel {row:?} expected{expected}"
            );
        }
    }
    assert_eq!(rows[24][3], 230.0);
    assert_eq!(rows[25][3], 231.0);
    assert_eq!(rows[26][3], 232.0);
    assert_eq!(rows[27][3], 234.0);
    // Independent 16,384-sample reference of the constant-radiance GGX
    // integral: normal/glancing smooth, oblique/glancing rough. A 16-sample
    // deterministic estimate stays within 1.5% of the albedo response scale.
    let integrated_goldens = [
        [0.10606261, 0.20458989, 0.3959814],
        [0.1402181, 0.27791643, 0.56599104],
        [0.075253256, 0.14542322, 0.28334603],
        [0.10530659, 0.20400159, 0.4002357],
    ];
    for (row, expected) in rows[29..33].iter().zip(integrated_goldens) {
        for ((actual, expected), tint) in row[..3].iter().zip(expected).zip([0.2, 0.4, 0.8]) {
            assert!(
                (actual - expected).abs() < 0.015 * tint,
                "GGX conductor environment {row:?} expected{expected}"
            );
            assert!(
                *actual >= 0.0 && *actual <= tint,
                "bounded conductor environment response"
            );
        }
    }
    for (channel, tint) in [0.01, 0.03, 0.05].into_iter().enumerate() {
        assert!(
            (rows[17][channel] - tint).abs() < 1e-6,
            "preset metal keeps albedo tint at grazing"
        );
        assert!(
            rows[16][channel] > 0.0 && rows[16][channel] < tint,
            "preset normal F0 includes measured conductor response"
        );
        assert!(
            (rows[18][channel] - 1.0).abs() < 1e-6,
            "generic metal uses albedo F0 and white grazing limit"
        );
        assert!(
            rows[19][channel] >= 0.0 && rows[19][channel] <= tint + 1e-6,
            "integrated environment and SSR response preserve full-lobe tint"
        );
    }
}
