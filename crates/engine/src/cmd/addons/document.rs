//! Document Tools (`addon.document.*`), a built-in add-on: Save a Timestamped Copy writes a copy of
//! the document named with the date and time through File › Save a Copy's save. Its submenu
//! (Object › Plug-ins › Document Tools) is the UI's.

#[cfg(test)]
mod tests;

use serde_json::{Value, json};

use super::super::fileio::{self, SaveMode};
use super::super::{CommandSpec, bad, clock_date, cmd, date_param, has_doc, str_param};
use crate::{Result, Session};

/// The most numbers (`-2`, `-3`…) tried after a timestamped name that is taken.
const MAX_NUMBER: u32 = 999;

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "addon.document.saveTimestampedCopy",
        "Save a Timestamped Copy",
        ["Object", "Plug-ins", "Document Tools"],
        None,
        "{folder? (default: the document's own folder; needed for a document never saved), format?: vectorcraft|pdf|svg|svgz|ai|eps (default: the document's own, else vectorcraft), time?: Unix seconds (default: now)} write a copy of the document named <name>_YYYY-MM-DD_HH-MM.<ext> (the date and time in UTC; -2, -3… added when that name is taken) as File › Save a Copy writes it: the document keeps its path, title and modified state, and no undo step is made. Off where there is no clock (the web) → {path, format, bytes, warnings}",
        has_clock,
        save_timestamped_copy
    )]
}

/// A document is open and the time can be told.
fn has_clock(s: &Session) -> std::result::Result<(), String> {
    has_doc(s)?;
    vectorcraft_doc::metadata::output_time().map(|_| ()).ok_or_else(|| "there is no clock here".into())
}

/// `<base>.<ext>`, then `<base>-2.<ext>`… in `folder`: the first that `taken` doesn't hold.
fn free_name(folder: &str, base: &str, ext: &str, taken: impl Fn(&str) -> bool) -> Option<String> {
    (1..=MAX_NUMBER)
        .map(|i| {
            let name = if i == 1 { format!("{base}.{ext}") } else { format!("{base}-{i}.{ext}") };
            std::path::Path::new(folder).join(name).to_string_lossy().into_owned()
        })
        .find(|path| !taken(path))
}

fn save_timestamped_copy(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "addon.document.saveTimestampedCopy";
    let st = s.doc()?;
    let format = match str_param(p, "format") {
        Some(f) => fileio::save_format(Some(f), None),
        // A document opened from a format Save doesn't write (a picture, a drawing) copies as native.
        None => fileio::save_format(Some(st.format), None).or_else(|_| fileio::save_format(None, None)),
    }
    .map_err(|e| bad(C, e))?;
    let folder = match str_param(p, "folder").filter(|f| !f.trim().is_empty()) {
        Some(f) => f.to_string(),
        None => st
            .path
            .as_deref()
            .and_then(|path| std::path::Path::new(path).parent())
            .map(|f| f.to_string_lossy().into_owned())
            .filter(|f| !f.is_empty())
            .ok_or_else(|| bad(C, "the document has never been saved: give `folder`"))?,
    };
    let stem = fileio::file_stem(st.path.as_deref().unwrap_or(&st.doc.title));
    let given = date_param(p, "time", C)?;
    let t = clock_date(s, "time", given).ok_or_else(|| bad(C, "there is no clock here: give `time` (Unix seconds)"))?;
    let [y, mo, d, h, mi, _] = vectorcraft_doc::metadata::civil(t);
    let base = format!("{stem}_{y:04}-{mo:02}-{d:02}_{h:02}-{mi:02}");
    // Only whether something is there: the write itself goes through the automation roots.
    let path = free_name(&folder, &base, format.extensions[0], |p| std::path::Path::new(p).exists())
        .ok_or_else(|| bad(C, "every numbered name for the copy is taken"))?;
    fileio::save_document(s, SaveMode::Copy, &json!({ "path": path, "format": format.id }))
}
