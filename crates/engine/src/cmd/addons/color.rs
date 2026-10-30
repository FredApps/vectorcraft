//! Color Effects (`addon.color.*`), a built-in add-on: its live effects (Exposure, Desaturate and
//! Invert Colors, in Effect › Color Effects) are the effects crate's; here, Apply as Filter writes
//! any colour adjustment into the colours themselves. Its panel and its submenu, Object › Plug-ins ›
//! Color Effects, are the UI's.

#[cfg(test)]
mod tests;

use serde_json::{Value, json};
use vectorcraft_render::effects;

use super::super::colorcmds::recolor;
use super::super::{CommandSpec, bad, checked_ids_or_selection, cmd, has_selection, text_param};
use crate::{Result, Session};

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "addon.color.filter",
        "Apply as Filter",
        ["Object", "Plug-ins", "Color Effects"],
        None,
        "{effect: a colour adjustment's effect id (adjust.brightnessContrast, adjust.curves, adjust.hueSaturation, adjust.levels, adjust.shiftToColor, adjust.temperatureTint, addon.color.exposure, addon.color.desaturate or addon.color.invert), params?: {…} (as effect.apply; missing keys take the dialog defaults), fill?: true, stroke?: true, includeImages?: true, includePatterns?: true, ids?: [id…] (default: the selection)} recolour the objects and everything inside them as the live effect would draw them, but in their colours, as edit.colors.invert does (fills, strokes, type, gradient meshes, embedded images and pattern tiles), as one undo step; their effects stay → {changed}",
        has_selection,
        filter
    )]
}

fn filter(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "addon.color.filter";
    let id = text_param(p, "effect", C)?.ok_or_else(|| bad(C, "missing `effect`"))?;
    if !effects::is_adjustment(id) {
        return Err(bad(C, format!("`{id}` is not a colour adjustment")));
    }
    let params = match p.get("params") {
        None | Some(Value::Null) => json!({}),
        Some(v @ Value::Object(_)) => v.clone(),
        Some(_) => return Err(bad(C, "`params` must be an object")),
    };
    let fx = effects::new_effect(id, &params).ok_or_else(|| bad(C, format!("unknown effect `{id}`")))?;
    let ids = checked_ids_or_selection(s, p, C)?;
    // Settings that change nothing still make the pass, which then leaves every colour as it was.
    let Some(map) = effects::color_map(&[fx]) else { return Ok(json!({ "changed": 0 })) };
    let mut given = p.clone();
    if let Some(o) = given.as_object_mut() {
        o.insert("ids".into(), json!(ids.iter().map(|i| i.0).collect::<Vec<_>>()));
    }
    let label = effects::effect_info(id).map_or("Apply as Filter", |e| e.label.trim_end_matches('…'));
    recolor(s, &given, label, &|c| map.apply(c))
}
