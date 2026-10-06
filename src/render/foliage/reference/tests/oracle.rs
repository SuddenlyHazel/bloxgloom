//! Optional dialect conversion of developer-supplied GLSL, independent of the port.
pub(super) fn source() -> String {
    let Some(original) = crate::render::bsl_reference::audit_source("lib/vertex/waving.glsl")
    else {
        return include_str!("portable_oracle.wgsl").to_owned();
    };
    let original = original.replace('\r', "");
    let function = |name: &str| {
        let start = original.find(&format!(" {name}(")).unwrap();
        let start = original[..start].rfind('\n').map_or(0, |at| at + 1);
        let end = start + original[start..].find("\n}").unwrap() + 2;
        original[start..end].to_owned()
    };
    let mut source = String::from(
        "var<private> source_time:f32;var<private> cameraPosition:vec3f;var<private> relativeEyePosition:vec3f;const ANIMATION_STRENGTH:f32=1.0;const pi:f32=3.1415927;\n",
    );
    for (name, header) in [
        ("GetNoise", "fn GetNoise(pos:vec2f)->f32 {"),
        ("Noise2D", "fn Noise2D(pos:vec2f)->f32 {"),
        (
            "CalcMove",
            "fn CalcMove(input:vec3f,density:f32,speed:f32,mult_in:vec2f)->vec3f {\nvar pos=input;var mult=mult_in;",
        ),
        (
            "CalcLilypadMove",
            "fn CalcLilypadMove(worldPos:vec3f)->f32 {",
        ),
        (
            "CalcGrassBend",
            "fn CalcGrassBend(input:vec3f)->vec3f {\nvar position=input;",
        ),
    ] {
        let glsl = function(name);
        let body = &glsl[glsl.find('{').unwrap() + 1..];
        source.push_str(header);
        source.push_str(body);
        source.push('\n');
    }
    let glsl = function("WavingBlocks");
    let body = &glsl[glsl.find('{').unwrap() + 1..glsl.find("    #ifdef WAVING_LAVA").unwrap()];
    source.push_str(
        "fn WavingBlocks(input:vec3f,blockID:u32,istopv:f32)->vec3f {var position=input;\n",
    );
    let lines: Vec<_> = body
        .lines()
        .filter(|line| !line.trim().starts_with('#'))
        .collect();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index].trim();
        index += 1;
        if line.starts_with("if ") && !line.contains('{') {
            let mut condition = line.to_owned();
            while condition.chars().filter(|&c| c == '(').count()
                > condition.chars().filter(|&c| c == ')').count()
            {
                condition.push(' ');
                condition.push_str(lines[index].trim());
                index += 1;
            }
            source.push_str(&condition);
            source.push_str(" {\n");
            source.push_str(lines[index]);
            source.push_str("\n}\n");
            index += 1;
        } else {
            source.push_str(line);
            source.push('\n');
        }
    }
    source.push_str("return position+wave;\n}\n");
    source = source
        .replace("vec2(", "vec2f(")
        .replace("vec3(", "vec3f(")
        .replace("time *", "source_time *")
        .replace(
            "pos * density + source_time * speed",
            "pos * density + vec3f(source_time * speed)",
        )
        .replace("(3.0 - 2.0 * frc)", "(vec2f(3.0) - 2.0 * frc)")
        .replace("pos.xz + 0.333", "pos.xz + vec2f(0.333)")
        .replace("pos.xy + 0.667", "pos.xy + vec2f(0.667)");
    for kind in ["vec2", "vec3", "float"] {
        source = source
            .lines()
            .map(|line| {
                let trimmed = line.trim_start();
                if let Some(rest) = trimmed.strip_prefix(&format!("{kind} ")) {
                    format!("var {rest}")
                } else {
                    line.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
    }
    source
}
