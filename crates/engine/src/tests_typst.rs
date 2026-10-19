//! Type › Typst… (`type.typst.insert`, `type.typst.edit`, `type.typst.info`) with the `typst`
//! program: each test is skipped when it isn't installed.

use serde_json::{Value, json};
use vectorcraft_doc::{NodeId, NodeKind};
use vectorcraft_geom::Rect;

use super::*;

/// Is the `typst` program there to run?
fn have_typst() -> bool {
    let program = std::env::var(cmd::typst::PROGRAM_VAR).unwrap_or_else(|_| "typst".into());
    let found = std::process::Command::new(program).arg("--version").output().is_ok_and(|o| o.status.success());
    if !found {
        eprintln!("typst isn't installed: skipped");
    }
    found
}

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 400, "height": 300})).unwrap();
    s
}

fn bounds(s: &Session, id: u64) -> Rect {
    let d = &s.doc().unwrap().doc;
    d.node(NodeId(id)).unwrap().geometric_bounds_with(&d.symbol_boxes()).unwrap()
}

fn id_of(r: &Value) -> u64 {
    r["id"].as_u64().unwrap()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-3
}

#[test]
fn insert_places_paths_that_keep_their_source() {
    if !have_typst() {
        return;
    }
    let mut s = session();
    let r = s.execute("type.typst.insert", &json!({"source": "$ x^2 + y^2 = z^2 $", "at": [100, 80]})).unwrap();
    let id = id_of(&r);
    let d = &s.doc().unwrap().doc;
    let g = d.node(NodeId(id)).unwrap();
    assert!(matches!(g.kind, NodeKind::Group { .. }));
    assert_eq!(g.kind_label(), "Typst");
    // Glyphs are plain paths: no symbols join the document.
    assert!(d.symbols.is_empty());
    let mut paths = 0;
    g.walk(&mut |n| {
        assert!(!matches!(n.kind, NodeKind::SymbolInstance { .. }));
        paths += matches!(n.kind, NodeKind::Path { .. } | NodeKind::Compound { .. }) as usize;
    });
    assert!(paths >= 9, "x 2 + y 2 = z 2: {paths} paths");
    assert_eq!(s.doc().unwrap().selection.objects, vec![NodeId(id)]);
    let b = bounds(&s, id);
    // Centred on `at` (the page is; the ink lies a little off its centre).
    assert!((b.center().x - 100.0).abs() < b.width() / 4.0 && (b.center().y - 80.0).abs() < b.height(), "{b:?}");
    let info = s.execute("type.typst.info", &json!({})).unwrap();
    assert_eq!(info["source"], "$ x^2 + y^2 = z^2 $");
    assert_eq!(info["fontSize"], 11.0);
    // One undo step.
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(s.doc().unwrap().doc.node(NodeId(id)).is_none());
}

#[test]
fn a_bigger_font_size_typesets_bigger() {
    if !have_typst() {
        return;
    }
    let mut s = session();
    let a = id_of(&s.execute("type.typst.insert", &json!({"source": "Hello"})).unwrap());
    let b = id_of(&s.execute("type.typst.insert", &json!({"source": "Hello", "fontSize": 22})).unwrap());
    let (a, b) = (bounds(&s, a).width(), bounds(&s, b).width());
    assert!((b / a - 2.0).abs() < 0.05, "{a} → {b}");
}

#[test]
fn edit_keeps_the_objects_place_scale_and_rotation() {
    if !have_typst() {
        return;
    }
    let mut s = session();
    let id = id_of(&s.execute("type.typst.insert", &json!({"source": "$ a $", "at": [200, 150]})).unwrap());
    s.execute("object.scale", &json!({"sx": 200})).unwrap();
    s.execute("object.move", &json!({"dx": 30, "dy": 10})).unwrap();
    let before = bounds(&s, id);
    let r = s.execute("type.typst.edit", &json!({"source": "$ a + b + c $"})).unwrap();
    let new = id_of(&r);
    let after = bounds(&s, new);
    // The centre stays; the art is twice the size it typesets at.
    assert!(close(after.center().y, before.center().y) && (after.center().x - before.center().x).abs() < 0.5, "{before:?} → {after:?}");
    let mut fresh = session();
    let plain = id_of(&fresh.execute("type.typst.insert", &json!({"source": "$ a + b + c $"})).unwrap());
    let natural = bounds(&fresh, plain);
    assert!((after.height() / natural.height() - 2.0).abs() < 0.01, "{natural:?} → {after:?}");
    // What isn't given stays: the edit is selected, and its info is the new source.
    assert_eq!(s.doc().unwrap().selection.objects, vec![NodeId(new)]);
    assert_eq!(s.execute("type.typst.info", &json!({})).unwrap()["source"], "$ a + b + c $");
    // Rotated, the edit stays rotated.
    s.execute("object.rotate", &json!({"angle": 90})).unwrap();
    let tall = bounds(&s, new);
    let r = s.execute("type.typst.edit", &json!({"fontSize": 11})).unwrap();
    let b = bounds(&s, id_of(&r));
    assert!(b.height() > b.width() && close(b.height(), tall.height()), "{tall:?} → {b:?}");
}

#[test]
fn the_source_is_saved_with_the_document() {
    if !have_typst() {
        return;
    }
    let mut s = session();
    let id = id_of(&s.execute("type.typst.insert", &json!({"source": "#emph[hi]", "preamble": "#set text(fill: red)", "fontSize": 14})).unwrap());
    let bytes = vectorcraft_format::save(&s.doc().unwrap().doc, false);
    let doc = vectorcraft_format::load(&bytes).unwrap();
    let t = doc.node(NodeId(id)).unwrap().extras().typst.clone().unwrap();
    assert_eq!((t.source.as_str(), t.preamble.as_str(), t.font_size), ("#emph[hi]", "#set text(fill: red)", 14.0));
}

#[test]
fn typst_errors_name_the_line_of_the_source() {
    if !have_typst() {
        return;
    }
    let mut s = session();
    let e = s.execute("type.typst.insert", &json!({"source": "ok\n$ x^ $", "preamble": "#let a = 1"})).unwrap_err().to_string();
    assert!(e.contains("line 2:"), "{e}");
    let e = s.execute("type.typst.insert", &json!({"source": "ok", "preamble": "#nope"})).unwrap_err().to_string();
    assert!(e.contains("preamble line 1:"), "{e}");
    // Nothing was added.
    assert!(s.execute("type.typst.info", &json!({})).is_err());
}

#[test]
fn bad_params_fail_before_typesetting() {
    let mut s = session();
    for p in [json!({}), json!({"source": "  "}), json!({"source": 3}), json!({"source": "a", "fontSize": 0}), json!({"source": "a", "at": [1]})] {
        assert!(s.execute("type.typst.insert", &p).is_err(), "{p}");
    }
    // Edit and info need a Typst object.
    assert!(s.execute("type.typst.edit", &json!({"source": "a"})).is_err());
    s.execute("shape.rectangle", &json!({"x": 10, "y": 10, "width": 20, "height": 20})).unwrap();
    assert!(s.execute("type.typst.info", &json!({})).is_err());
}
