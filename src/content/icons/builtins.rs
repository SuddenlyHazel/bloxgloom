//! The remaining original HUD art. Imported materials use their texture icons.
use bloxgloom_host_api::icon::ItemIcon;

pub(super) fn definitions() -> Vec<ItemIcon> {
    vec![ItemIcon {
        item: "bloxgloom:seeds".into(),
        rows: [
            "....ssss....",
            "...stttss...",
            "..stuuuttss..",
            "..sttttstss..",
            "...sssstss...",
            "......ss.....",
            "..ssss.......",
            ".stttss.......",
            ".stuutss......",
            "..sstsss......",
            "....sss.......",
            "..............",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
        palette: vec![
            (b's', [0.35, 0.23, 0.15, 1.0]),
            (b't', [0.72, 0.48, 0.24, 1.0]),
            (b'u', [0.92, 0.72, 0.40, 1.0]),
        ],
    }]
}
