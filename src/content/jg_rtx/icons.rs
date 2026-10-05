//! Small source-art thumbnails shared by the HUD and egui inventory. The import
//! script projects cube faces or silhouettes before bounded palette reduction.
use super::Catalog;
use bloxgloom_host_api::icon::ItemIcon;
use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Deserialize)]
struct Thumbnails {
    icons: Vec<Thumbnail>,
}
#[derive(Deserialize)]
struct Thumbnail {
    key: String,
    rows: Vec<String>,
    palette: Vec<(u8, [f32; 4])>,
}

pub(super) fn register(catalog: &mut Catalog) {
    static THUMBNAILS: OnceLock<Thumbnails> = OnceLock::new();
    let thumbnails = THUMBNAILS.get_or_init(|| {
        serde_json::from_str(include_str!("../../../assets/jg-rtx/icons.json"))
            .expect("checked-in imported item thumbnails")
    });
    for thumbnail in &thumbnails.icons {
        catalog
            .register_item_icon(ItemIcon {
                item: format!("bloxgloom:{}", thumbnail.key),
                rows: thumbnail.rows.clone(),
                palette: thumbnail.palette.clone(),
            })
            .expect("unique bounded imported thumbnail with registered item");
    }
}
