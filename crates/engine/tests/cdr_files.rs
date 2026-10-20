//! Local, original CorelDRAW-authored oracles. No proprietary artwork, fonts or profiles are
//! committed. Set CDR_FIXTURE_DIR as described in docs/cdr-import.md to run these checks.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use vectorcraft_engine::cmd::fileio;
use vectorcraft_render::Renderer;

#[test]
/// Compare every page of optional original CorelDRAW fixtures with independently exported SVGs.
fn original_corel_files_match_their_independent_svg_exports() {
    let Ok(folder) = std::env::var("CDR_FIXTURE_DIR") else { return };
    let folder = Path::new(&folder);
    let mut cases = Vec::new();
    for stem in ["rectangle", "geometry"] {
        for version in [15, 16, 27] {
            cases.push((stem, version, if stem == "rectangle" { 1 } else { 2 }));
        }
    }
    for stem in ["gradient", "group-transform"] {
        if folder.join(format!("{stem}-v27.cdr")).exists() {
            cases.push((stem, 27, 1));
        }
    }
    for version in [15, 16, 27] {
        if folder.join(format!("bitmap-v{version}.cdr")).exists() {
            cases.push(("bitmap", version, 1));
        }
    }
    for (stem, version, expected_pages) in cases {
        let name = format!("{stem}-v{version}.cdr");
        let bytes = std::fs::read(folder.join(&name)).unwrap();
        let loaded = fileio::load(&name, &bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(loaded.format.id, "cdr");
        assert!(!loaded.preview_only);
        if stem == "bitmap" {
            assert_eq!(loaded.doc.images.len(), 2, "both embedded artwork images must be retained");
        } else {
            assert!(loaded.doc.images.is_empty());
        }
        assert_eq!(loaded.doc.artboards.len(), expected_pages, "{name}");
        for (page, ab) in loaded.doc.artboards.iter().enumerate() {
            let oracle_name = format!("{stem}-page{}.svg", page + 1);
            let oracle = std::fs::read_to_string(folder.join(&oracle_name)).unwrap();
            let reference = vectorcraft_svg::import(&oracle).unwrap();
            let frame = reference.artboards[0].rect;
            assert!((ab.rect.width() - frame.width()).abs() < 0.01, "{name} page width");
            assert!((ab.rect.height() - frame.height()).abs() < 0.01, "{name} page height");
            let actual = Renderer::new().render_region(&loaded.doc, ab.rect, 1.0, stem != "bitmap");
            let expected = Renderer::new().render_region(&reference, frame, 1.0, stem != "bitmap");
            assert_eq!((actual.width, actual.height), (expected.width, expected.height));
            let mean = actual.pixels.iter().zip(&expected.pixels).map(|(a, b)| a.abs_diff(*b) as f64).sum::<f64>() / actual.pixels.len() as f64;
            eprintln!("{name} page {}: mean byte difference {mean:.6}; warnings={:?}", page + 1, loaded.warnings);
            if let Ok(output) = std::env::var("CDR_RENDER_DIR") {
                std::fs::create_dir_all(&output).unwrap();
                std::fs::write(Path::new(&output).join(format!("{stem}-v{version}-page{}.png", page + 1)), actual.to_png().unwrap()).unwrap();
                std::fs::write(Path::new(&output).join(format!("{stem}-reference-page{}.png", page + 1)), expected.to_png().unwrap()).unwrap();
            }
            if stem == "bitmap" {
                // Corel applies its CMYK profile when exporting PNG. Import keeps continuous
                // alpha and numeric profile-free colour, so compare geometry/alpha separately.
                let alpha_mean = actual
                    .pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .zip(expected.pixels.as_chunks::<4>().0.iter())
                    .map(|(a, b)| a[3].abs_diff(b[3]) as f64)
                    .sum::<f64>()
                    / (actual.pixels.len() / 4) as f64;
                assert!(alpha_mean < 0.1, "{name}: bitmap placement/alpha differs from Corel: {alpha_mean}");
                let actual_images: Vec<_> = loaded.doc.images.values().map(|b| image::load_from_memory(&b.bytes).unwrap().to_rgba8()).collect();
                let expected_images: Vec<_> = reference.images.values().map(|b| image::load_from_memory(&b.bytes).unwrap().to_rgba8()).collect();
                let original_pixels = image::open(folder.join("bitmap-original-checker.png")).unwrap().to_rgba8();
                assert!(actual_images.contains(&original_pixels), "native RGB/alpha must preserve the original generated PNG exactly");
                assert!(
                    actual_images.iter().any(|a| {
                        expected_images.iter().any(|b| {
                            a.dimensions() == b.dimensions()
                                && a.pixels().zip(b.pixels()).all(|(a, b)| {
                                    // Corel rounds straight RGB when exporting partial alpha and
                                    // replaces invisible RGB at alpha zero. Coverage is exact;
                                    // opaque colour is exact; premultiplied partial colour is
                                    // allowed one byte of export quantization.
                                    a[3] == b[3]
                                        && (0..3).all(|c| {
                                            if a[3] == 255 {
                                                a[c] == b[c]
                                            } else {
                                                let alpha = u32::from(a[3]);
                                                let pa = (u32::from(a[c]) * alpha + 127) / 255;
                                                let pb = (u32::from(b[c]) * alpha + 127) / 255;
                                                pa.abs_diff(pb) <= 1
                                            }
                                        })
                                })
                        })
                    }),
                    "RGB bitmap pixels/alpha must match the independent SVG export within export quantization"
                );
                assert!(loaded.warnings.iter().any(|w| w.contains("CMYK bitmap")));
            } else {
                assert!(mean < 0.1, "{name} page {} differs from the CorelDRAW SVG oracle: {mean:.6}", page + 1);
            }
        }
    }
}
