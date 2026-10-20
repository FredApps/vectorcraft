#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use proptest::prelude::*;
use vectorcraft_color::Paint;
use vectorcraft_doc::Node;
use vectorcraft_geom::Shape;

/// Collect editable path nodes recursively for fixture geometry assertions.
fn paths<'a>(node: &'a Node, out: &mut Vec<&'a Node>) {
    if node.path_data().is_some() {
        out.push(node);
    }
    if let Some(c) = node.children() {
        for node in c {
            paths(node, out);
        }
    }
}

#[test]
/// Verify generated geometry, page size and numeric fill/outline properties.
fn original_rectangle_is_editable_geometry_with_numeric_paint() {
    let bytes = synth::rectangle();
    let imported = import(&bytes).unwrap();
    assert_eq!(imported.version, 1200);
    assert_eq!(imported.document.artboards.len(), 1);
    let rect = imported.document.artboards[0].rect;
    assert!((rect.width() - 100.0).abs() < 0.001);
    let mut objects = Vec::new();
    for n in &imported.document.layers {
        paths(n, &mut objects);
    }
    assert_eq!(objects.len(), 1);
    let bounds = objects[0].path_data().unwrap().to_bezpath().bounding_box();
    assert!((bounds.width() - 40.0).abs() < 0.001);
    assert!((bounds.height() - 20.0).abs() < 0.001);
    assert_eq!(objects[0].appearance.fill_paint(), vectorcraft_color::Paint::solid(vectorcraft_color::Color::rgb8(255, 0, 0)));
    assert!((objects[0].appearance.stroke_width() - 1.0).abs() < 0.001);
    assert!(imported.warnings.is_empty(), "{:?}", imported.warnings);
}

#[test]
/// Refuse every incomplete prefix of an otherwise valid original drawing.
fn every_truncated_prefix_is_rejected_without_panic() {
    let bytes = synth::rectangle();
    for n in 0..bytes.len() {
        assert!(import(&bytes[..n]).is_err(), "accepted prefix {n}");
    }
}

#[test]
/// Preserve the original bitmap samples, top-down placement and native crop as separate objects.
fn embedded_bitmap_keeps_pixels_placement_and_editable_crop() {
    use vectorcraft_doc::NodeKind;
    let bytes = synth::clipped_bitmap();
    let imported = import(&bytes).unwrap();
    assert!(imported.warnings.is_empty(), "{:?}", imported.warnings);
    let doc = imported.document;
    assert_eq!(doc.images.len(), 1);
    let page = &doc.layers[0];
    let layer = &page.children().unwrap()[0];
    assert!(layer.clips());
    let children = layer.children().unwrap();
    assert!(matches!(children[0].kind, NodeKind::Path { rule: vectorcraft_geom::FillRule::EvenOdd, clipping: true, .. }));
    assert!(children[0].appearance.items.is_empty(), "a bitmap crop must not paint behind transparent pixels");
    let NodeKind::Image(image) = &children[1].kind else { panic!("the crop's content must remain an image") };
    assert_eq!((image.width, image.height), (2, 2));
    let top_left = image.xf * vectorcraft_geom::Point::new(0.0, 0.0);
    let bottom_right = image.xf * vectorcraft_geom::Point::new(2.0, 2.0);
    assert!((top_left.x - 50.0).abs() < 0.001 && (top_left.y - 40.0).abs() < 0.001);
    assert!((bottom_right.x - 90.0).abs() < 0.001 && (bottom_right.y - 60.0).abs() < 0.001);
    let crop = children[0].path_data().unwrap().to_bezpath();
    assert!(crop.contains(vectorcraft_geom::Point::new(55.0, 45.0)));
    assert!(!crop.contains(vectorcraft_geom::Point::new(85.0, 55.0)));
    let pixels = image::load_from_memory(&doc.images[&image.key].bytes).unwrap().to_rgba8();
    assert_eq!(pixels.get_pixel(0, 0).0, [255, 0, 0, 128]);
    assert_eq!(pixels.get_pixel(1, 0).0, [0, 255, 0, 128]);
    assert_eq!(pixels.get_pixel(0, 1).0, [0, 0, 255, 128]);
    assert_eq!(pixels.get_pixel(1, 1).0, [255, 255, 255, 128]);
}

#[test]
/// Versions 6/7 have a 20-byte bitmap tail; only version 8 uses the shorter 12-byte tail.
fn older_bitmap_objects_keep_their_native_crop_tail() {
    for version in [600, 700] {
        let (mut chunks, _) = crate::container::load(&synth::clipped_bitmap()).unwrap();
        let config = chunks.iter_mut().find(|c| c.id == *b"mcfg").unwrap();
        let size = config.data[4..].to_vec();
        config.data = vec![0; if version == 600 { 28 } else { 0 }];
        config.data.extend(size);
        let imported = crate::drawing::import(&chunks, version).unwrap();
        let crop = &imported.document.layers[0].children().unwrap()[0];
        assert!(crop.clips(), "version {version} must retain the image crop");
        assert_eq!(crop.children().unwrap()[0].path_data().unwrap().subpaths.len(), 2, "both outer crop and hole survive");
    }
}

#[test]
/// Check optional local CorelDRAW originals against their known geometry and paint values.
fn real_corel_files_when_supplied_locally() {
    let Ok(path) = std::env::var("CDR_FIXTURE_DIR") else { return };
    for file in [
        "rectangle-v15.cdr",
        "rectangle-v16.cdr",
        "rectangle-v27.cdr",
        "geometry-v15.cdr",
        "geometry-v16.cdr",
        "geometry-v27.cdr",
        "gradient-v27.cdr",
        "group-transform-v27.cdr",
        "color-models-v27.cdr",
    ] {
        let fixture = std::path::Path::new(&path).join(file);
        if !(file.starts_with("rectangle") || file.starts_with("geometry")) && !fixture.exists() {
            continue;
        }
        let bytes = std::fs::read(fixture).unwrap();
        if std::env::var_os("CDR_DUMP").is_some() {
            /// Print bounded record prefixes for explicitly enabled local fixture diagnostics.
            fn dump(chunks: &[crate::container::Chunk], depth: usize) {
                for c in chunks {
                    eprintln!(
                        "{}{} {:?} {} {:02x?}",
                        " ".repeat(depth),
                        String::from_utf8_lossy(&c.id),
                        c.list.map(|v| String::from_utf8_lossy(&v).into_owned()),
                        c.data.len(),
                        c.data.get(..c.data.len().min(64)).unwrap_or_default()
                    );
                    if &c.id == b"fild" {
                        eprintln!("FILLTAIL {:02x?}", c.data.get(c.data.len().saturating_sub(64)..).unwrap_or_default());
                    }
                    dump(&c.children, depth + 1);
                }
            }
            dump(&crate::container::load(&bytes).unwrap().0, 0);
        }
        let imported = import(&bytes).unwrap_or_else(|e| panic!("{file}: {e}"));
        let mut objects = Vec::new();
        for n in &imported.document.layers {
            paths(n, &mut objects);
        }
        let count = if file.starts_with("geometry") {
            4
        } else if file.starts_with("gradient") || file.starts_with("group-transform") || file.starts_with("color-models") {
            2
        } else {
            1
        };
        assert_eq!(objects.len(), count, "{file}: {:?}", imported.warnings);
        let mm = 72.0 / 25.4;
        if file.starts_with("rectangle") || file.starts_with("geometry") {
            let red = objects.iter().find(|n| n.name.as_deref() == Some("Red rectangle")).unwrap();
            let b = red.path_data().unwrap().to_bezpath().bounding_box();
            assert!((b.x0 - 9.962 * mm).abs() < 0.001, "{file}: {b:?}");
            assert!((b.y0 - 20.038 * mm).abs() < 0.001, "{file}: {b:?}");
            assert!((b.width() - 40.0 * mm).abs() < 0.001);
            assert!((red.appearance.stroke_width() - mm).abs() < 0.001);
            assert_eq!(red.appearance.fill_paint(), vectorcraft_color::Paint::solid(vectorcraft_color::Color::rgb8(255, 0, 0)));
        }
        if file.starts_with("gradient") {
            let Paint::Gradient(linear) = objects[0].appearance.fill_paint() else { panic!("missing linear gradient") };
            let Paint::Gradient(radial) = objects[1].appearance.fill_paint() else { panic!("missing radial gradient") };
            assert_eq!(linear.gradient.kind, vectorcraft_color::GradientKind::Linear);
            let l = linear.geom.unwrap();
            assert!((l.start.x - 10.0 * mm).abs() < 0.001);
            assert!((l.end.x - 90.0 * mm).abs() < 0.001);
            let r = radial.geom.unwrap();
            assert!((r.start.x - 50.0 * mm).abs() < 0.001);
            assert!((r.start.y - 75.0 * mm).abs() < 0.001);
            assert!((r.length() - 40.0 * mm).abs() < 0.001);
            assert!((r.aspect - 0.375).abs() < 1e-8);
            assert_eq!(radial.gradient.stops[0].color, vectorcraft_color::Color::WHITE);
        }
        if file.starts_with("group-transform") {
            assert_eq!(objects[0].name.as_deref(), Some("Red group child"));
            let b = objects[0].path_data().unwrap().to_bezpath().bounding_box();
            assert!((b.x0 - 22.3526 * mm).abs() < 0.001);
            assert!((b.y0 - 42.1132 * mm).abs() < 0.001);
            assert!((b.x1 - 63.6603 * mm).abs() < 0.001);
            assert!((b.y1 - 73.6603 * mm).abs() < 0.001);
        }
        if file.starts_with("color-models") {
            assert_eq!(objects[0].appearance.fill_paint(), Paint::solid(vectorcraft_color::Color::cmyk(1.0, 0.0, 0.0, 0.0)));
            assert_eq!(objects[1].appearance.fill_paint(), Paint::solid(vectorcraft_color::Color::gray(1.0 - 128.0 / 255.0)));
        }
        if std::env::var_os("CDR_DUMP").is_some() {
            for o in objects {
                eprintln!(
                    "{:?} {:?} fill={:?} stroke={:?}",
                    o.name,
                    o.path_data().unwrap().to_bezpath().bounding_box(),
                    o.appearance.fill_paint(),
                    o.appearance.stroke()
                );
            }
        }
    }
}

#[test]
/// Give an actionable failure when every object has an unsupported native kind.
fn unsupported_only_artwork_has_an_actionable_error() {
    let mut bytes = synth::rectangle();
    let at = bytes.windows(4).position(|p| p == b"loda").unwrap() + 8 + 16;
    bytes[at..at + 4].copy_from_slice(&4u32.to_le_bytes());
    assert!(import(&bytes).unwrap_err().contains("convert text to curves"));
}

#[test]
/// Refuse an unsupported clipping-only drawing instead of claiming an empty import succeeded.
fn clipping_only_artwork_is_not_reported_as_a_blank_drawing() {
    let mut bytes = synth::rectangle();
    let start = bytes.windows(4).position(|p| p == b"obj ").unwrap() - 8;
    let len = u32::from_le_bytes(bytes[start + 4..start + 8].try_into().unwrap()) as usize;
    let inner = bytes[start..start + 8 + len].to_vec();
    let mut outer = b"LIST".to_vec();
    outer.extend_from_slice(&((inner.len() + 4) as u32).to_le_bytes());
    outer.extend_from_slice(b"clpt");
    outer.extend(inner);
    bytes.splice(start..start + 8 + len, outer);
    let page = bytes.windows(4).position(|p| p == b"page").unwrap() - 4;
    let n = u32::from_le_bytes(bytes[page..page + 4].try_into().unwrap()) + 12;
    bytes[page..page + 4].copy_from_slice(&n.to_le_bytes());
    let n = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) + 12;
    bytes[4..8].copy_from_slice(&n.to_le_bytes());
    assert!(import(&bytes).unwrap_err().contains("no supported editable artwork"));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]
    #[test]
    /// Keep arbitrary bounded input on the fallible parsing path.
    fn arbitrary_input_never_panics(bytes in proptest::collection::vec(any::<u8>(),0..8192)) {
        let _ = is_cdr(&bytes); let _ = import(&bytes);
    }
    #[test]
    /// Reject or import byte mutations of an original generated record structure without panics.
    fn mutations_of_real_record_structure_never_panic(offset in 0usize..512, value in any::<u8>()) {
        let mut bytes = synth::rectangle();
        if let Some(b) = bytes.get_mut(offset) { *b = value; }
        let _ = import(&bytes);
    }
}
