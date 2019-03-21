use std::path::Path;
use std::fs::File;
use std::io::Read;
use app_units::Au;

use webrender::api::*;

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

pub fn init_font(api: &RenderApi, pipeline_id: PipelineId, document_id: DocumentId, font_size: i32) -> (FontKey, FontInstanceKey) {
    let mut txn = Transaction::new();
    txn.set_root_pipeline(pipeline_id);
    let font_key = add_font(&api, &mut txn, "resources/Fira Code/ttf/FiraCode-Medium.ttf");
    let font_instance_key = add_font_instance(&api, &mut txn, font_key, font_size);
    api.send_transaction(document_id, txn);
    return (font_key, font_instance_key);
}

pub fn layout_simple_ascii(
    api: &RenderApi,
    font_key: FontKey,
    instance_key: FontInstanceKey,
    text: &str,
    size: Au,
    origin: LayoutPoint,
    flags: FontInstanceFlags,
) -> (Vec<u32>, Vec<LayoutPoint>, LayoutRect) {
    let indices: Vec<u32> = api
        .get_glyph_indices(font_key, text)
        .iter()
        .filter_map(|idx| *idx)
        .collect();

    let metrics = api.get_glyph_dimensions(instance_key, indices.clone());

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
                let space_advance = size.to_f32_px() / 3.0;
                cursor += horizontal_direction * space_advance;
            }
        }
    }

    let bounding_rect = bounding_rect.inflate(2.0, 2.0);

    (indices, positions, bounding_rect)
}


pub fn show_text(api: &RenderApi,
                 font_key: FontKey,
                 text_size: i32,
                 font_instance_key: FontInstanceKey,
                 builder: &mut DisplayListBuilder,
                 space_and_clip: &SpaceAndClipInfo,
                 text: &str,
                 origin: LayoutPoint) {
    let mut line_origin = origin;
    let vertical_direction = LayoutVector2D::new(0.0, Au::from_px(text_size).to_f32_px() + 4.0);
    for line in text.split_terminator("\n") {
        let (indices, positions, bounding_rect) = layout_simple_ascii(&api,
                                                                      font_key,
                                                                      font_instance_key,
                                                                      line,
                                                                      Au::from_px(text_size),
                                                                      line_origin,
                                                                      FontInstanceFlags::default());
        let glyphs: Vec<GlyphInstance> = indices.iter().zip(positions)
            .map(|(idx, pos)| GlyphInstance { index: *idx, point: pos })
            .collect();
        let info = LayoutPrimitiveInfo::new(bounding_rect);
        builder.push_text(
            &info,
            &space_and_clip,
            glyphs.as_slice(),
            font_instance_key,
            ColorF::BLACK,
            None,
        );
        line_origin += vertical_direction;
    }
//    for g in glyphs {
//        builder.push_rect(&LayoutPrimitiveInfo::new(LayoutRect::new(g.point, euclid::TypedSize2D::new(3.0, 3.0))), space_and_clip, ColorF::BLACK);
//    }

}