use std::path::{Path, PathBuf};
use std::fs::File;
use std::io::Read;

use webrender::api::*;
use fxhash::FxHashMap;
use std::sync::Arc;
use crate::text_layout;

pub struct Font {
    pub font_key: FontKey,
    pub font: font_kit::font::Font,
    pub hb_font: text_layout::HbFace
}

struct FontInstance {
    font_instance_key: FontInstanceKey,
    size: f32
}

pub fn load_font<P: AsRef<Path>>(api: &RenderApi, txn: &mut Transaction, path: P) -> Font {
    let font_index = 0; // 0 for single font file
    let mut file = File::open(path).unwrap();
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).unwrap();
    let font_key = api.generate_font_key();
    txn.add_raw_font(font_key, buffer.clone(), font_index);
    let font = font_kit::handle::Handle::from_memory(Arc::new(buffer), font_index).load().unwrap();
    let hb_font = text_layout::HbFace::new(&font);
    Font {
        font_key: font_key,
        font: font,
        hb_font: hb_font
    }
}

pub fn add_font_instance(api: &RenderApi, txn: &mut Transaction, font_key: FontKey, size: i32) -> FontInstanceKey {
    let font_instance_key = api.generate_font_instance_key();
    txn.add_font_instance(font_instance_key,
                          font_key,
                          app_units::Au::from_px(size),
                          None,
                          None,
                          Vec::new());
    return font_instance_key;
}


#[derive(Debug)]
pub struct LayoutedText {
    pub glyphs: Vec<GlyphInstance>,
    pub size: LayoutSize,
}

pub struct FontsManager {
    api: RenderApi,
    pub font: Font,
    pub font_instance_key: FontInstanceKey,
    pub font_size: f32,
}

impl FontsManager {
    pub fn new(api: RenderApi, document_id: DocumentId) -> Self {
        let font_size = 14.0;
        let mut txn = Transaction::new();
        let font = load_font(&api, &mut txn, "resources/Fira Code/ttf/FiraCode-Retina.ttf");
        let font_instance_key = add_font_instance(&api, &mut txn, font.font_key, font_size as i32);
        api.send_transaction(document_id, txn);
        FontsManager {
            api,
            font,
            font_instance_key,
            font_size,
        }
    }

    pub fn layout_simple_ascii(
        &mut self,
        text: &str) -> LayoutedText {
        return crate::text_layout::layout_run(&self.font, text, self.font_size);
    }
}
