use super::*;
use serde_json::{Value, json};

struct Fixture {
    doc: Value,
    data: Vec<u8>,
}
impl Fixture {
    fn accessor(&mut self, values: &[f32], kind: &str, width: usize) -> usize {
        let view = self.doc["bufferViews"].as_array().unwrap().len();
        let offset = self.data.len();
        for value in values {
            self.data.extend(value.to_le_bytes());
        }
        self.doc["bufferViews"]
            .as_array_mut()
            .unwrap()
            .push(json!({"buffer":0,"byteOffset":offset,"byteLength":values.len()*4}));
        let index = self.doc["accessors"].as_array().unwrap().len();
        self.doc["accessors"].as_array_mut().unwrap().push(
            json!({"bufferView":view,"componentType":5126,"count":values.len()/width,"type":kind}),
        );
        index
    }
    fn new() -> Self {
        let mut f = Self {
            doc: json!({"asset":{"version":"2.0"},"scene":0,"scenes":[{"nodes":[0]}],
            "nodes":[{"name":"root","children":[1,2,4]}, {"name":"body","mesh":0},
                {"name":"eyes_classic","children":[3],"mesh":0}, {"name":"iris","mesh":0}, {"name":"eyes_sleepy","mesh":0}],
            "meshes":[],"materials":[{"name":"body_color","pbrMetallicRoughness":{"baseColorFactor":[0.5,0.25,0.1,1]}}],
            "bufferViews":[],"accessors":[],"buffers":[{"byteLength":0}],"animations":[]}),
            data: Vec::new(),
        };
        let pos = f.accessor(&[-0.5, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 1.0, 0.0], "VEC3", 3);
        f.doc["accessors"][pos]["min"] = json!([-0.5, 0, 0]);
        f.doc["accessors"][pos]["max"] = json!([0.5, 1, 0]);
        let normals = f.accessor(&[0.0, 0.0, -1.0, 0.0, 0.0, -1.0, 0.0, 0.0, -1.0], "VEC3", 3);
        f.doc["meshes"] =
            json!([{"primitives":[{"attributes":{"POSITION":pos,"NORMAL":normals},"material":0}]}]);
        let times = f.accessor(&[0.0, 1.0], "SCALAR", 1);
        let values = f.accessor(&[0.0, 0.0, 0.0, 2.0, 0.0, 0.0], "VEC3", 3);
        f.doc["animations"] = json!([{"name":"walk","samplers":[{"input":times,"output":values,"interpolation":"LINEAR"}],
            "channels":[{"sampler":0,"target":{"node":1,"path":"translation"}}]}]);
        f
    }
    fn bytes(&mut self) -> Vec<u8> {
        self.doc["buffers"][0]["byteLength"] = json!(self.data.len());
        let mut json = serde_json::to_vec(&self.doc).unwrap();
        while !json.len().is_multiple_of(4) {
            json.push(b' ');
        }
        let mut bin = self.data.clone();
        while !bin.len().is_multiple_of(4) {
            bin.push(0);
        }
        let length = 12 + 8 + json.len() + 8 + bin.len();
        let mut bytes = Vec::new();
        for v in [
            0x46546c67_u32,
            2,
            length as u32,
            json.len() as u32,
            0x4e4f534a,
        ] {
            bytes.extend(v.to_le_bytes());
        }
        bytes.extend(json);
        bytes.extend((bin.len() as u32).to_le_bytes());
        bytes.extend(0x004e4942_u32.to_le_bytes());
        bytes.extend(bin);
        bytes
    }
}
fn controls() -> Controls {
    serde_json::from_value(json!({"variants":[{"name":"eyes","default":"classic","options":[
        {"name":"classic","nodes":["eyes_classic"]},{"name":"sleepy","nodes":["eyes_sleepy"]}]}],
        "layers":[{"name":"body","nodes":["body"],"visible":true}],
        "tints":[{"name":"skin","materials":["body_color"],"color":{"rgb":[255,128,0],"mode":"multiply"}}],
        "loops":{"walk":true}})).unwrap()
}
#[test]
fn native_glb_preserves_named_clips_and_customizable_subtrees() {
    let model = Model::from_glb(&Fixture::new().bytes(), controls()).unwrap();
    assert_eq!(model.clips[0].name, "walk");
    assert_eq!(model.sample(Some("walk"), 0.5).unwrap()[1].w_axis.x, 1.0);
    assert_eq!(model.sample(Some("walk"), 1.5).unwrap()[1].w_axis.x, 1.0);
    let a = model.appearance(&Look::default()).unwrap();
    assert_eq!(a.visible, vec![true, true, true, true, false]);
    assert!((a.colors[0][1] - 0.21586).abs() < 0.0001); // decode sRGB exactly once
    let look = serde_json::from_value(json!({"variants":{"eyes":"sleepy"},"layers":{"body":false},
        "tints":{"skin":{"rgb":[32,255,128],"mode":"replace"}}}))
    .unwrap();
    let a = model.appearance(&look).unwrap();
    assert_eq!(a.visible, vec![true, false, false, false, true]);
    assert_eq!(a.colors[0][3], 1.0);
    assert!(model.sample(Some("unknown"), 0.0).is_err());
    assert!(
        model
            .appearance(&serde_json::from_value(json!({"variants":{"eyes":"unknown"}})).unwrap())
            .is_err()
    );
    assert!(
        model
            .appearance(&serde_json::from_value(json!({"tints":{"typo":{"rgb":[0,0,0]}}})).unwrap())
            .is_err()
    );
}
#[test]
fn native_glb_rejects_bad_accessors_cycles_and_external_resources() {
    for mutation in 0..5 {
        let mut f = Fixture::new();
        match mutation {
            0 => f.doc["bufferViews"][0]["byteOffset"] = json!(999999),
            1 => f.doc["accessors"][0]["count"] = json!(999999),
            2 => f.doc["nodes"][3]["children"] = json!([0]),
            3 => f.doc["buffers"][0]["uri"] = json!("private.bin"),
            _ => {
                f.doc["images"] = json!([{"uri":"private.png"}]);
            }
        }
        assert!(
            Model::from_glb(&f.bytes(), Controls::default()).is_err(),
            "mutation {mutation}"
        );
    }
    let mut bad = controls();
    bad.layers[0].nodes = vec!["missing".into()];
    assert!(Model::from_glb(&Fixture::new().bytes(), bad).is_err());
    let mut bad = controls();
    bad.variants[0].options[1].nodes = vec!["eyes_classic".into()];
    assert!(Model::from_glb(&Fixture::new().bytes(), bad).is_err());
}
#[test]
fn native_glb_decodes_embedded_png_and_weighted_skins() {
    let mut f = Fixture::new();
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, 1, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[120, 80, 40, 255])
            .unwrap();
    }
    let image_view = f.doc["bufferViews"].as_array().unwrap().len();
    f.doc["bufferViews"]
        .as_array_mut()
        .unwrap()
        .push(json!({"buffer":0,"byteOffset":f.data.len(),"byteLength":png.len()}));
    f.data.extend(png);
    while !f.data.len().is_multiple_of(4) {
        f.data.push(0);
    }
    f.doc["images"] = json!([{"bufferView":image_view,"mimeType":"image/png"}]);
    f.doc["textures"] = json!([{"source":0}]);
    f.doc["materials"][0]["pbrMetallicRoughness"]["baseColorTexture"] = json!({"index":0});
    let weights = f.accessor(
        &[0.5, 0.5, 0.0, 0.0, 0.5, 0.5, 0.0, 0.0, 0.5, 0.5, 0.0, 0.0],
        "VEC4",
        4,
    );
    let js = f.accessor(&[0.0; 12], "VEC4", 4);
    f.doc["accessors"][js]["componentType"] = json!(5123);
    let offset = f.doc["bufferViews"]
        [f.doc["accessors"][js]["bufferView"].as_u64().unwrap() as usize]["byteOffset"]
        .as_u64()
        .unwrap() as usize;
    for i in 0..3 {
        for (j, v) in [0_u16, 1, 0, 0].iter().enumerate() {
            f.data[offset + i * 8 + j * 2..offset + i * 8 + j * 2 + 2]
                .copy_from_slice(&v.to_le_bytes());
        }
    }
    f.doc["meshes"][0]["primitives"][0]["attributes"]["JOINTS_0"] = json!(js);
    f.doc["meshes"][0]["primitives"][0]["attributes"]["WEIGHTS_0"] = json!(weights);
    f.doc["skins"] = json!([{"joints":[0,2]}]);
    f.doc["nodes"][1]["skin"] = json!(0);
    f.doc["nodes"][2]["translation"] = json!([2, 0, 0]);
    let model = Model::from_glb(&f.bytes(), Controls::default()).unwrap();
    assert_eq!(model.images[0].rgba, [120, 80, 40, 255]);
    let vertex = model
        .primitives
        .iter()
        .find(|p| p.node == 1)
        .unwrap()
        .vertices[0];
    let pose = model.sample(None, 0.0).unwrap();
    let x: f32 = (0..4)
        .map(|i| pose[vertex.joints[i] as usize].w_axis.x * vertex.weights[i])
        .sum();
    assert_eq!(x, 1.0);
}
