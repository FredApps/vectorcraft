//! CDR uses the same open/place path as other editable formats, with read-only source files.

use serde_json::json;
use vectorcraft_doc::NodeKind;

use super::*;

#[test]
/// Expose CDR in shared open/place filters while keeping the source format read-only.
fn cdr_is_registered_for_open_and_place_but_never_for_saving() {
    let mut s = Session::new();
    let r = s.execute("document.formats", &json!({})).unwrap();
    let f = r["formats"].as_array().unwrap().iter().find(|f| f["id"] == "cdr").unwrap();
    assert_eq!(f["read"], true);
    assert_eq!(f["write"], false);
    assert_eq!(f["raster"], false);
    assert_eq!(format_for_name("DRAWING.CDR").unwrap().id, "cdr");
    assert!(OPEN_EXTS.contains(&"cdr") && PLACE_EXTS.contains(&"cdr"));
    assert!(open_filters().any(|(label, exts)| label == "CorelDRAW" && exts.contains(&"cdr")));
    assert!(place_filters().any(|(label, exts)| label == "CorelDRAW" && exts.contains(&"cdr")));
    assert!(!SAVE_FORMATS.contains(&"cdr"));
    assert!(writable_format(Some("cdr"), None).is_err());
}

#[test]
/// Recognize native CDR data without stealing unrelated ZIP or RIFF inputs.
fn cdr_sniffing_preserves_other_zip_and_riff_formats() {
    let wave = b"RIFF\x04\0\0\0WAVE";
    assert!(detect("sound.wav", wave).is_none());
    let zip = zip::store(&[("ordinary.txt", b"text")]).unwrap();
    assert!(detect("archive.zip", &zip).is_none());
    let unrelated_root = zip::store(&[("content/root.dat", wave.as_slice())]).unwrap();
    assert!(detect("other.zip", &unrelated_root).is_none());
    assert_eq!(detect("unknown.bin", &vectorcraft_cdr::synth::rectangle()).unwrap().id, "cdr");
}

#[test]
/// Route legacy diagnostics through `.cdr` names while preserving other extension fallbacks.
fn cdr_waldo_diagnostics_use_the_extension_without_hijacking_other_formats() {
    let bytes = b"WL unrelated content";
    assert!(detect("unknown.bin", bytes).is_none());
    assert_eq!(detect("drawing.svg", bytes).unwrap().id, "svg");
    assert_eq!(detect("legacy.cdr", bytes).unwrap().id, "cdr");
    let mut s = Session::new();
    assert!(open_bytes(&mut s, "legacy.cdr", bytes, None).unwrap_err().to_string().contains("Waldo"));
}

#[test]
/// Preserve editable geometry through native serialization and protect the imported CDR source.
fn cdr_open_keeps_editable_vectors_and_requires_a_new_save_path() {
    let bytes = vectorcraft_cdr::synth::rectangle();
    let mut s = Session::new();
    let r = open_bytes(&mut s, "drawing.cdr", &bytes, Some("drawing.cdr".into())).unwrap();
    assert_eq!(r["format"], "cdr");
    let st = s.doc().unwrap();
    assert!(st.path.is_none(), "Save must not overwrite a CDR source");
    assert!(st.doc.images.is_empty(), "native geometry, no preview raster");
    assert!(st.doc.node_count() >= 2);
    let svg = vectorcraft_svg::export(&st.doc, &vectorcraft_svg::ExportOptions::default());
    assert!(svg.contains("<path"), "the rectangle is editable path geometry");
    assert!(!svg.contains("<image"));
    let native = s.execute("document.serialize", &json!({"format": "vectorcraft"})).unwrap();
    let data = vectorcraft_format::base64_decode(native["dataBase64"].as_str().unwrap()).unwrap();
    open_bytes(&mut s, "copy.vectorcraft", &data, None).unwrap();
    assert!(s.doc().unwrap().doc.node_count() >= 2);
    assert!(s.execute("document.export", &json!({"format": "cdr"})).is_err());
}

#[test]
/// Place editable CDR geometry as one group that a single undo removes.
fn cdr_place_is_an_editable_group_and_one_undo_step() {
    let bytes = vectorcraft_cdr::synth::rectangle();
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 400, "height": 300})).unwrap();
    let before = s.doc().unwrap().doc.node_count();
    let r =
        s.execute("file.place", &json!({"name": "drawing.cdr", "dataBase64": vectorcraft_format::base64_encode(&bytes), "at": [200, 150]})).unwrap();
    assert_eq!(r["format"], "cdr");
    assert_eq!(r["linked"], false);
    assert!(s.doc().unwrap().doc.node_count() > before);
    assert!(matches!(s.doc().unwrap().doc.layers[0].children().unwrap().last().unwrap().kind, NodeKind::Group { .. }));
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().doc.node_count(), before);
}

#[test]
/// Retain embedded image data and its crop through rendering, native save/reopen, place and undo.
fn cdr_bitmap_crops_survive_render_native_round_trip_and_place() {
    let bytes = vectorcraft_cdr::synth::clipped_bitmap();
    let mut s = Session::new();
    open_bytes(&mut s, "bitmap.cdr", &bytes, Some("bitmap.cdr".into())).unwrap();
    let doc = &s.doc().unwrap().doc;
    assert_eq!(doc.images.len(), 1);
    assert!(s.doc().unwrap().path.is_none());
    let frame = doc.artboards[0].rect;
    let before = vectorcraft_render::Renderer::new().render_region(doc, frame, 1.0, false);
    assert!(before.pixels.as_chunks::<4>().0.iter().any(|p| p[3] != 0));
    assert_eq!(before.pixels[(45 * 100 + 65) * 4 + 3], 128, "the crop must not paint a background beneath translucent image pixels");
    assert_eq!(before.pixels[(55 * 100 + 85) * 4 + 3], 0, "the cropped-away corner stays transparent");
    assert_eq!(before.pixels[(43 * 100 + 56) * 4 + 3], 0, "a same-winding crop subpath cuts a real hole");
    let saved = s.execute("document.serialize", &json!({"format": "vectorcraft"})).unwrap();
    let native = vectorcraft_format::base64_decode(saved["dataBase64"].as_str().unwrap()).unwrap();
    open_bytes(&mut s, "bitmap-copy.vectorcraft", &native, None).unwrap();
    let doc = &s.doc().unwrap().doc;
    assert_eq!(doc.images.len(), 1);
    let after = vectorcraft_render::Renderer::new().render_region(doc, frame, 1.0, false);
    assert_eq!(before.pixels, after.pixels);
    s.execute("file.new", &json!({"width": 400, "height": 300})).unwrap();
    let count = s.doc().unwrap().doc.node_count();
    s.execute("file.place", &json!({"name": "bitmap.cdr", "dataBase64": vectorcraft_format::base64_encode(&bytes)})).unwrap();
    assert_eq!(s.doc().unwrap().doc.images.len(), 1);
    assert!(s.doc().unwrap().doc.node_count() > count);
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().doc.node_count(), count);
}

#[test]
/// Keep the active document and tab count intact after malformed CDR opens fail.
fn malformed_cdr_does_not_replace_the_active_document() {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 40, "height": 30})).unwrap();
    s.execute("shape.rectangle", &json!({"x": 1, "y": 1, "width": 5, "height": 5})).unwrap();
    let before = s.doc().unwrap().doc.node_count();
    let count = s.documents().len();
    for bytes in [b"not a CDR".as_slice(), b"RIFF\xff\xff\xff\xffCDRL".as_slice(), b"PK\x03\x04".as_slice()] {
        assert!(open_bytes(&mut s, "broken.cdr", bytes, None).is_err());
        assert_eq!(s.documents().len(), count);
        assert_eq!(s.doc().unwrap().doc.node_count(), before);
    }
}
