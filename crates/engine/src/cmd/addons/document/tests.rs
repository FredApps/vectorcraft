//! Document Tools' commands: the copy's name and place, the document left as it was, and bad
//! params refused.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::Session;

/// 2026-10-05 14:03:00 UTC.
const TIME: i64 = 1_791_208_980;

/// A new, empty folder of its own for test `name`.
fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vc-addon-document-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 400, "height": 300})).unwrap();
    s.execute("shape.rectangle", &json!({"x": 10, "y": 10, "width": 100, "height": 50})).unwrap();
    s
}

fn stamp(s: &mut Session, p: Value) -> crate::Result<Value> {
    s.execute("addon.document.saveTimestampedCopy", &p)
}

#[test]
fn a_timestamped_copy_goes_next_to_the_document_which_stays_as_it_was() {
    let dir = folder("next-to");
    let mut s = session();
    let own = dir.join("poster.vectorcraft");
    s.execute("document.save", &json!({"path": own})).unwrap();
    s.execute("shape.ellipse", &json!({"x": 0, "y": 0, "width": 10, "height": 10})).unwrap();
    let (steps, title) = (s.doc().unwrap().history.undo.len(), s.doc().unwrap().doc.title.clone());
    let r = stamp(&mut s, json!({"time": TIME})).unwrap();
    let copy = dir.join("poster_2026-10-05_14-03.vectorcraft");
    assert_eq!(Path::new(r["path"].as_str().unwrap()), copy);
    assert!(copy.is_file());
    let st = s.doc().unwrap();
    assert_eq!(st.path.as_deref().map(Path::new), Some(own.as_path()), "the document keeps its path");
    assert_eq!((st.history.undo.len(), &st.doc.title, st.is_dirty()), (steps, &title, true), "no step; still modified");
    // The copy holds the document as it is now: the ellipse too.
    let mut other = Session::new();
    other.execute("document.open", &json!({"path": copy})).unwrap();
    assert_eq!(other.doc().unwrap().doc.layers[0].children().unwrap().len(), 2);
    // The same minute again: numbered.
    let r = stamp(&mut s, json!({"time": TIME})).unwrap();
    assert!(r["path"].as_str().unwrap().ends_with("poster_2026-10-05_14-03-2.vectorcraft"), "{r}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_unsaved_document_needs_a_folder_and_other_formats_take_their_extension() {
    let dir = folder("unsaved");
    let mut s = session();
    let before = s.doc().unwrap().doc.clone();
    assert!(stamp(&mut s, json!({"time": TIME})).is_err(), "no folder to put it in");
    let r = stamp(&mut s, json!({"time": TIME, "folder": dir, "format": "svg"})).unwrap();
    let name = Path::new(r["path"].as_str().unwrap()).file_name().unwrap().to_string_lossy().into_owned();
    assert!(name.ends_with("_2026-10-05_14-03.svg") && dir.join(&name).is_file(), "{name}");
    let st = s.doc().unwrap();
    assert!(st.path.is_none() && *st.doc == *before, "the document is unchanged");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bad_params_are_refused_and_writes_stay_inside_the_automation_roots() {
    let dir = folder("bad");
    let mut s = session();
    for p in [
        json!({"folder": dir, "format": "png"}),
        json!({"folder": dir, "format": "dxf"}),
        json!({"folder": dir, "time": "now"}),
        json!({"folder": dir, "time": 1.5}),
        json!({"folder": dir.join("missing").join("deeper"), "time": TIME}),
    ] {
        assert!(stamp(&mut s, p.clone()).is_err(), "{p}");
    }
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0, "nothing written");
    let allowed = dir.join("allowed");
    std::fs::create_dir_all(&allowed).unwrap();
    let roots = crate::file_access::AutomationRoots::new(&[&dir], &[&allowed]).unwrap();
    let outside = crate::file_access::confine(roots.as_ref(), || stamp(&mut s, json!({"folder": dir, "time": TIME})));
    assert!(outside.is_err(), "outside the write root");
    let inside = crate::file_access::confine(roots.as_ref(), || stamp(&mut s, json!({"folder": allowed, "time": TIME})));
    assert!(inside.is_ok(), "{inside:?}");
    assert!(!enabled_without_document(), "off with no document");
    let _ = std::fs::remove_dir_all(&dir);
}

fn enabled_without_document() -> bool {
    Session::new().commands().into_iter().find(|c| c.id == "addon.document.saveTimestampedCopy").unwrap().enabled
}
