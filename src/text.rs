use std::path::Path;
use std::fs::File;
use std::io::Read;

use webrender::api::*;
use fxhash::FxHashMap;

pub fn add_font<P: AsRef<Path>>(api: &RenderApi, txn: &mut Transaction, path: P) -> FontKey {

    let mut file = File::open(path).unwrap();
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).unwrap();
    let font_key = api.generate_font_key();
    txn.add_raw_font(font_key, buffer, 0);
    return font_key;
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
    pub bounding_rect: LayoutRect,
}

pub struct FontsManager {
    api: RenderApi,
    pub font_key: FontKey,
    pub font_instance_key: FontInstanceKey,
    pub font_size: f32,
    pub index_cache: FxHashMap<char, Option<u32>>,
    pub metrics_cache: FxHashMap<u32, Option<GlyphDimensions>>,
}

impl FontsManager {
    pub fn new(api: RenderApi, document_id: DocumentId) -> Self {
        let font_size = 14.0;
        let mut txn = Transaction::new();
        let font_key = add_font(&api, &mut txn, "resources/Fira Code/ttf/FiraCode-Medium.ttf");
        let font_instance_key = add_font_instance(&api, &mut txn, font_key, font_size as i32);
        api.send_transaction(document_id, txn);
        FontsManager {
            api,
            font_key,
            font_instance_key,
            font_size,
            index_cache: FxHashMap::default(),
            metrics_cache: FxHashMap::default(),
        }
    }

    fn get_glyph_indices(&mut self, text: &str) -> Vec<Option<u32>> {
        let mut indexes = vec![Default::default(); text.len()];
        let mut missed_chars = String::new();
        let mut miss_positions = Vec::new();
        for (pos, ch) in text.chars().enumerate() {
            if let Some(idx) = self.index_cache.get(&ch) {
                indexes[pos] = *idx;
            } else {
                missed_chars.push(ch);
                miss_positions.push(pos);
            }
        }

        if missed_chars.len() > 0 {
            let missing_indexes = self.api.get_glyph_indices(self.font_key, missed_chars.as_str());
            for (pos, index) in miss_positions.iter().zip(missing_indexes.iter()) {
                indexes[*pos] = *index;
            }
            for (index, ch) in missing_indexes.iter().zip(missed_chars.chars().into_iter()) {
                self.index_cache.insert(ch, *index);
            }
        }
        indexes
    }

    fn get_glyph_dimensions(&mut self, indexes: &Vec<GlyphIndex>) -> Vec<Option<GlyphDimensions>> {
        let mut glyph_dimensions = vec![Default::default(); indexes.len()];
        let mut missed_indexes = Vec::new();
        let mut miss_positions = Vec::new();
        for (pos, index) in indexes.iter().enumerate() {
            if let Some(dimensions) = self.metrics_cache.get(&index) {
                glyph_dimensions[pos] = *dimensions;
            } else {
                missed_indexes.push(*index);
                miss_positions.push(pos);
            }
        }
        if missed_indexes.len() > 0 {
            let missing_dimensions = self.api.get_glyph_dimensions(self.font_instance_key, missed_indexes.clone());
            for (pos, dimensions) in miss_positions.iter().zip(missing_dimensions.iter()) {
                glyph_dimensions[*pos] = *dimensions;
            }
            for (dimensions, index) in missing_dimensions.iter().zip(missed_indexes.iter()) {
                self.metrics_cache.insert(*index, *dimensions);
            }
        }
        glyph_dimensions
    }

    pub fn layout_simple_ascii(
        &mut self,
        text: &str,
        origin: LayoutPoint,
        flags: FontInstanceFlags,
    ) -> LayoutedText {
        let indices: Vec<u32> = self.get_glyph_indices(text).iter().filter_map(|idx| *idx).collect();

        let metrics = self.get_glyph_dimensions(&indices);

        let mut bounding_rect = LayoutRect::zero();
        let mut positions = Vec::new();

        let mut cursor = origin;
        let horizontal_direction = if flags.contains(FontInstanceFlags::TRANSPOSE) {
            LayoutVector2D::new(
                0.0,
                if flags.contains(FontInstanceFlags::FLIP_Y) { -1.0 } else { 1.0 },
            )
        } else {
            LayoutVector2D::new(
                if flags.contains(FontInstanceFlags::FLIP_X) { -1.0 } else { 1.0 },
                0.0,
            )
        };

        for (_ch, metric) in text.chars().zip(metrics) {
            positions.push(cursor);

            match metric {
                Some(metric) => {
                    let glyph_rect = LayoutRect::new(
                        LayoutPoint::new(cursor.x + metric.left as f32, cursor.y - metric.top as f32),
                        LayoutSize::new(metric.width as f32, metric.height as f32)
                    );
                    bounding_rect = bounding_rect.union(&glyph_rect);
                    cursor += horizontal_direction * metric.advance;
                }
                None => {
                    let space_advance = self.font_size;
                    cursor += horizontal_direction * space_advance;
                }
            }
        }

        let bounding_rect = bounding_rect.inflate(2.0, 2.0);
        let glyphs: Vec<GlyphInstance> = indices.iter().zip(positions)
            .map(|(idx, pos)| GlyphInstance { index: *idx, point: pos })
            .collect();
        LayoutedText {
            glyphs,
            bounding_rect
        }
    }

    pub fn show_text(&mut self,
                     builder: &mut DisplayListBuilder,
                     space_and_clip: &SpaceAndClipInfo,
                     text: &str,
                     origin: LayoutPoint) {
        let mut line_origin = origin;
        let vertical_direction = LayoutVector2D::new(0.0, self.font_size + 4.0);
        for line_text in text.split_terminator("\n") {
            let LayoutedText { glyphs, bounding_rect } = self.layout_simple_ascii(line_text,
                                                                                  line_origin,
                                                                                  FontInstanceFlags::default());

            let info = LayoutPrimitiveInfo::new(bounding_rect);
            builder.push_text(
                &info,
                &space_and_clip,
                glyphs.as_slice(),
                self.font_instance_key,
                ColorF::BLACK,
                None,
            );
            line_origin += vertical_direction;
        }
//    for g in glyphs {
//        builder.push_rect(&LayoutPrimitiveInfo::new(LayoutRect::new(g.point, euclid::TypedSize2D::new(3.0, 3.0))), space_and_clip, ColorF::BLACK);
//    }

    }
}