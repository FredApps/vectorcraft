//! Color Effects: its live effects through effect.apply, and Apply as Filter.

use serde_json::json;
use vectorcraft_doc::NodeId;
use vectorcraft_geom::Rect;

use crate::Session;

/// A document with a selected red rectangle, unstroked.
fn red_rect() -> (Session, NodeId) {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 200, "height": 200})).unwrap();
    let id = NodeId(s.execute("shape.rectangle", &json!({"x": 20, "y": 20, "width": 60, "height": 60})).unwrap()["id"].as_u64().unwrap());
    s.execute("paint.setFill", &json!({"color": "#ff0000"})).unwrap();
    s.execute("paint.setStroke", &json!({"none": true})).unwrap();
    (s, id)
}

fn fill_hex(s: &Session, id: NodeId) -> String {
    s.doc().unwrap().doc.node(id).unwrap().appearance.fill_paint().color().unwrap().to_hex()
}

fn pixel(s: &Session) -> [u8; 4] {
    vectorcraft_render::Renderer::new().render_region(&s.doc().unwrap().doc, Rect::new(0.0, 0.0, 100.0, 100.0), 1.0, true).pixel(50, 50)
}

#[test]
fn the_live_effects_draw_without_changing_the_colours() {
    for (effect, params, drawn) in [
        ("addon.color.invert", json!({}), [0, 255, 255, 255]),
        ("addon.color.desaturate", json!({}), [128, 128, 128, 255]),
        ("addon.color.exposure", json!({"exposure": -20}), [0, 0, 0, 255]),
    ] {
        let (mut s, id) = red_rect();
        s.execute("effect.apply", &json!({"effect": effect, "params": params})).unwrap();
        assert_eq!(fill_hex(&s, id), "#ff0000", "{effect}");
        assert_eq!(pixel(&s), drawn, "{effect}");
    }
}

#[test]
fn a_filter_recolours_the_object_as_one_undo_step() {
    let (mut s, id) = red_rect();
    let undo = s.doc().unwrap().history.undo.len();
    let v = s.execute("addon.color.filter", &json!({"effect": "addon.color.invert"})).unwrap();
    assert_eq!(v["changed"], 1);
    assert_eq!(fill_hex(&s, id), "#00ffff");
    let n = s.doc().unwrap().doc.node(id).unwrap().clone();
    assert!(n.appearance.effects.is_empty(), "no effect is added");
    assert_eq!(s.doc().unwrap().history.undo.len(), undo + 1);
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(fill_hex(&s, id), "#ff0000");
}

#[test]
fn a_filter_takes_any_colour_adjustment_with_its_params() {
    let (mut s, id) = red_rect();
    s.execute("addon.color.filter", &json!({"effect": "adjust.hueSaturation", "params": {"hue": 120}})).unwrap();
    assert_eq!(fill_hex(&s, id), "#00ff00");
    // Live effects already on the object stay live, on top of the new colours.
    s.execute("effect.apply", &json!({"effect": "addon.color.invert"})).unwrap();
    s.execute("addon.color.filter", &json!({"effect": "addon.color.desaturate", "ids": [id.0]})).unwrap();
    assert_eq!(fill_hex(&s, id), "#808080");
    assert_eq!(s.doc().unwrap().doc.node(id).unwrap().appearance.effects.len(), 1);
    // The stroke only: the fill stays.
    s.execute("addon.color.filter", &json!({"effect": "addon.color.invert", "fill": false})).unwrap();
    assert_eq!(fill_hex(&s, id), "#808080");
}

#[test]
fn bad_requests_are_refused() {
    let (mut s, id) = red_rect();
    let undo = s.doc().unwrap().history.undo.len();
    for p in [
        json!({}),
        json!({"effect": 3}),
        json!({"effect": "distort.roughen"}),
        json!({"effect": "adjust.nothing"}),
        json!({"effect": "addon.color.invert", "params": [1]}),
        json!({"effect": "addon.color.invert", "ids": [999_999]}),
        json!({"effect": "addon.color.invert", "ids": ["x"]}),
    ] {
        assert!(s.execute("addon.color.filter", &p).is_err(), "{p}");
    }
    assert_eq!(fill_hex(&s, id), "#ff0000");
    assert_eq!(s.doc().unwrap().history.undo.len(), undo);
    s.execute("select.none", &json!({})).unwrap();
    assert!(s.execute("addon.color.filter", &json!({"effect": "addon.color.invert"})).is_err(), "nothing selected");
}
