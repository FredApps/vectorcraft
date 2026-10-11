//! The Rectangular Grid tool's click dialog. Rows and columns are divider counts, not cells.
//! Only a successful creation saves the counts for the next click or drag; Cancel and errors do
//! not change the tool settings. Dimensions start at 100 pt, like the other shape click dialogs.

use serde_json::{Value, json};
use vectorcraft_doc::Unit;
use vectorcraft_geom::Point;

use super::{DialogResult, DialogSpec, form};
use crate::VectorcraftApp;
use crate::state::Dialog;

pub(super) const KIND: &str = "rectangularGrid";

pub(super) const SPEC: DialogSpec = DialogSpec { heading: |_| tl!("Rectangular Grid").into(), body, confirm, ..DialogSpec::FORM };

pub(super) fn open(app: &mut VectorcraftApp, at: &Value) {
    let options = app.session.tool_options_of(KIND);
    app.ui.dialog = Some(Dialog::new(
        KIND,
        json!({
            "x": at.get("x").cloned().unwrap_or(json!(0)),
            "y": at.get("y").cloned().unwrap_or(json!(0)),
            "width": 100, "height": 100,
            "rows": options["rows"], "columns": options["columns"],
        }),
    ));
}

fn body(app: &mut VectorcraftApp, ui: &mut egui::Ui, d: &mut Dialog) -> bool {
    // Keep the entered text until OK, including invalid input: a bad value must not silently
    // revert to an earlier number when Enter confirms the dialog. Explicit units win over the
    // document's general unit, just as in the shared numeric fields.
    let unit = app.session.general_unit();
    egui::Grid::new("rectangular-grid").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
        length_field(ui, d, "width", tl!("Width:"), unit);
        length_field(ui, d, "height", tl!("Height:"), unit);
        for (key, label) in [("rows", tl!("Horizontal dividers:")), ("columns", tl!("Vertical dividers:"))] {
            form::field(ui, d, key, label);
        }
    });
    false
}

/// Display numeric point values in the document unit without rounding their stored value. Once
/// edited, the raw text stays in the field until confirmation, even if it cannot be parsed.
fn length_field(ui: &mut egui::Ui, d: &mut Dialog, key: &str, label: &str, unit: Unit) {
    let original = d.fields.get(key).cloned();
    let shown = original.as_ref().and_then(Value::as_f64).map(|v| json!(unit.format(v)));
    if let Some(shown) = &shown {
        d.fields.insert(key.into(), shown.clone());
    }
    let t = crate::theme::Tokens::get(ui.ctx());
    ui.label(egui::RichText::new(label).color(t.text_dim));
    let changed = form::text_edit(ui, d, key, 120.0).changed();
    ui.end_row();
    // Replacing a selection with the same rounded display is still an edit: use that entered
    // length, rather than restoring the original, unrounded point value.
    if shown.is_some()
        && !changed
        && let Some(original) = original
    {
        d.fields.insert(key.into(), original);
    }
}

/// Numeric JSON is already in points; unqualified length text uses the document's unit. The
/// click coordinates remain in document points. No default substitutes for an invalid field.
fn length(d: &Dialog, key: &str, unit: Unit) -> Option<f64> {
    match d.fields.get(key)? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => unit.parse(s),
        _ => None,
    }
    .filter(|v| v.is_finite())
}

/// Counts are plain numbers, never lengths. Whole floating-point JSON values are accepted and
/// emitted as integers, because the shape command and tool settings read integer JSON counts.
fn count(d: &Dialog, key: &str) -> Option<u32> {
    match d.fields.get(key)? {
        Value::Number(n) => {
            let v = n.as_f64()?;
            if v.is_finite() && v.fract() == 0.0 && (0.0..=999.0).contains(&v) { Some(v as u32) } else { None }
        }
        Value::String(s) => {
            let s = s.trim();
            // Parsing a float first would round a fraction such as 999.00000000000000001
            // into a valid count. Text counts use only decimal digits.
            if s.bytes().all(|b| b.is_ascii_digit()) { s.parse::<u32>().ok().filter(|v| *v <= 999) } else { None }
        }
        _ => None,
    }
}

fn confirm(app: &mut VectorcraftApp, d: &Dialog) -> DialogResult {
    let at = length(d, "x", Unit::Points).zip(length(d, "y", Unit::Points));
    let Some((x, y)) = at else { return Err("Grid position must be finite.".into()) };
    let unit = app.session.general_unit();
    let size = length(d, "width", unit).zip(length(d, "height", unit));
    let Some((width, height)) = size.filter(|(w, h)| *w > 0.0 && *h > 0.0 && (x + w).is_finite() && (y + h).is_finite()) else {
        return Err("Grid width and height must be positive, finite lengths.".into());
    };
    let Some((rows, columns)) = count(d, "rows").zip(count(d, "columns")) else {
        return Err("Grid divider counts must be whole numbers from 0 to 999.".into());
    };
    let params = json!({"x": x, "y": y, "width": width, "height": height, "rows": rows, "columns": columns});
    let (command, params) = vectorcraft_engine::perspective_click(&app.session, "shape.rectangularGrid", &params, Point::new(x, y))
        .unwrap_or_else(|| ("shape.rectangularGrid".into(), params));
    // Perspective attachment can fail after creating the flat grid (for example beyond the
    // horizon). A single-step batch rolls that creation back too, and keeps one undo step on OK.
    let result = app.run("command.batch", json!({"label": "Rectangular Grid", "commands": [{"command": command, "params": params}]}))?;
    // This known tool accepts both validated counts; setting its options adds no document undo
    // step. In particular, never save them before an insertion into the current layer succeeds.
    app.run("tool.setOption", json!({"tool": KIND, "values": {"rows": rows, "columns": columns}}))?;
    app.ui.dialog = None;
    Ok(result["results"][0].clone())
}
