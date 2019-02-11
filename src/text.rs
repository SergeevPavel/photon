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
    println!("{:?}", buffer.len());
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
    let direction = if flags.contains(FontInstanceFlags::TRANSPOSE) {
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
    for metric in metrics {
        positions.push(cursor);

        match metric {
            Some(metric) => {
                let glyph_rect = LayoutRect::new(
                    LayoutPoint::new(cursor.x + metric.left as f32, cursor.y - metric.top as f32),
                    LayoutSize::new(metric.width as f32, metric.height as f32)
                );
                bounding_rect = bounding_rect.union(&glyph_rect);
                cursor += direction * metric.advance;
            }
            None => {
                // Extract the advances from the metrics. The get_glyph_dimensions API
                // has a limitation that it can't currently get dimensions for non-renderable
                // glyphs (e.g. spaces), so just use a rough estimate in that case.
                let space_advance = size.to_f32_px() / 3.0;
                cursor += direction * space_advance;
            }
        }
    }

    let bounding_rect = bounding_rect.inflate(2.0, 2.0);

    (indices, positions, bounding_rect)
}