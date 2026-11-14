//! Smart sections (Figdesk): a section that carries a mode and a Play button. Play hands the section
//! to the AI agent with the mode's prompt document, which lives in the project's work folder
//! (`prompts/<mode>.md`); the agent builds what the document asks from the section's content. The
//! mark is the node's plugin data `figdesk.smart` = `{"mode": "web"}`, so it is saved with the file
//! and travels with copies of the section, and the file format has nothing new in it.
//!
//! A mode's name is also the name of its prompt file, so it is limited to letters, digits, `-` and
//! `_`: a document from someone else can't point it at another folder.

use figcraft_doc::{Node, NodeId, NodeKind};
use serde_json::{Value, json};

use super::CommandSpec;
use crate::params::{num_or, req_targets, string, targets};
use crate::{Result, Session, bad, spec};

/// The plugin-data key that marks a section as smart.
pub const KEY: &str = "figdesk.smart";
/// The mode a new smart section starts in.
pub const DEFAULT_MODE: &str = "web";
/// Longest mode name.
pub const MAX_MODE_LEN: usize = 32;

/// Whether `mode` can be a mode name (and so a prompt file's name).
pub fn valid_mode(mode: &str) -> bool {
    !mode.is_empty() && mode.len() <= MAX_MODE_LEN && mode.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// The mode of `node` when it is a smart section; a mark with a mode that isn't one (a damaged or
/// foreign file) reads as the default mode.
pub fn mode_of(node: &Node) -> Option<String> {
    if !matches!(node.kind, NodeKind::Section { .. }) {
        return None;
    }
    let mark = node.plugin_data.get(KEY)?;
    let mode = serde_json::from_str::<Value>(mark).ok().and_then(|v| v.get("mode").and_then(Value::as_str).map(str::to_string));
    Some(mode.filter(|m| valid_mode(m)).unwrap_or_else(|| DEFAULT_MODE.to_string()))
}

fn mode_param(cmd: &str, p: &Value) -> Result<Option<String>> {
    match string(p, "mode") {
        None => Ok(None),
        Some(m) if valid_mode(&m) => Ok(Some(m)),
        Some(m) => Err(bad(cmd, format!("`mode` must be 1 to {MAX_MODE_LEN} letters, digits, - or _ (got `{m}`)"))),
    }
}

fn is_section(s: &Session, id: NodeId) -> bool {
    s.doc().ok().and_then(|d| d.node(id)).is_some_and(|n| matches!(n.kind, NodeKind::Section { .. }))
}

fn mark(s: &mut Session, id: NodeId, mode: &str) -> Result<()> {
    s.doc_mut()?.get_mut(id)?.plugin_data.insert(KEY.into(), json!({"mode": mode}).to_string());
    Ok(())
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        spec!(
            "smartSection.create",
            "New smart section",
            "{ids? (sections to make smart: default the selected ones, when they are all sections), mode? (default web), x?, y?, width? (default 640), height? (default 480), name?, parent?} → {ids, mode}  a section with a mode and a Play button; without sections to convert it makes a new one, centred in the view and selected",
            true,
            create_smart
        ),
        spec!(
            "smartSection.setMode",
            "Set smart section mode",
            "{ids?, mode (letters, digits, - and _: the name of a prompt file in the work folder's prompts folder)} → {ids, mode}  makes the sections smart when they are not",
            true,
            set_mode
        ),
        spec!("smartSection.clear", "Make a normal section", "{ids?} → {ids}  removes the mode and the Play button", true, clear),
    ]
}

fn create_smart(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "smartSection.create";
    let mode = mode_param(cmd, p)?.unwrap_or_else(|| DEFAULT_MODE.to_string());
    let chosen = targets(s, p);
    let ids = if !chosen.is_empty() && chosen.iter().all(|id| is_section(s, *id)) {
        chosen
    } else {
        let (w, h) = (num_or(p, "width", 640.0), num_or(p, "height", 480.0));
        let view = &s.ds()?.view;
        let (x, y) = (num_or(p, "x", view.cx - w / 2.0), num_or(p, "y", view.cy - h / 2.0));
        let mut params =
            json!({"type": "section", "x": x, "y": y, "width": w, "height": h, "name": string(p, "name").unwrap_or_else(|| "Smart section".into())});
        if let (Some(parent), Some(o)) = (p.get("parent").filter(|v| !v.is_null()), params.as_object_mut()) {
            o.insert("parent".into(), parent.clone());
        }
        let made = super::create::create(s, &params)?;
        let id = made.get("id").and_then(Value::as_u64).map(NodeId).ok_or_else(|| bad(cmd, "the section was not made"))?;
        vec![id]
    };
    for id in &ids {
        mark(s, *id, &mode)?;
    }
    Ok(json!({"ids": ids, "mode": mode}))
}

fn set_mode(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "smartSection.setMode";
    let mode = mode_param(cmd, p)?.ok_or_else(|| bad(cmd, "`mode` is required"))?;
    let ids: Vec<NodeId> = req_targets(cmd, s, p)?.into_iter().filter(|id| is_section(s, *id)).collect();
    if ids.is_empty() {
        return Err(bad(cmd, "select a section"));
    }
    for id in &ids {
        mark(s, *id, &mode)?;
    }
    Ok(json!({"ids": ids, "mode": mode}))
}

fn clear(s: &mut Session, p: &Value) -> Result<Value> {
    let ids: Vec<NodeId> = targets(s, p).into_iter().filter(|id| mode_of_id(s, *id).is_some()).collect();
    for id in &ids {
        s.doc_mut()?.get_mut(*id)?.plugin_data.remove(KEY);
    }
    Ok(json!({"ids": ids}))
}

fn mode_of_id(s: &Session, id: NodeId) -> Option<String> {
    s.doc().ok().and_then(|d| d.node(id)).and_then(mode_of)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_names_are_file_names_that_cannot_leave_their_folder() {
        for ok in ["web", "web-2", "My_Mode", "a"] {
            assert!(valid_mode(ok), "{ok}");
        }
        for bad in ["", "../x", "a/b", "a.b", "a b", "ñ", &"x".repeat(MAX_MODE_LEN + 1), "..", "/etc"] {
            assert!(!valid_mode(bad), "{bad:?}");
        }
    }

    #[test]
    fn a_new_smart_section_is_a_marked_section_in_the_middle_of_the_view() {
        let mut s = Session::new();
        let r = s.execute("smartSection.create", &json!({})).expect("created");
        assert_eq!(r["mode"], "web");
        let id = NodeId(r["ids"][0].as_u64().expect("an id"));
        let node = s.doc().expect("doc").node(id).expect("node");
        assert_eq!(mode_of(node).as_deref(), Some("web"));
        assert_eq!((node.width, node.height, node.name.as_str()), (640.0, 480.0, "Smart section"));
        assert_eq!(s.selection(), vec![id]);
        s.execute("edit.undo", &json!({})).expect("undone");
        assert!(s.doc().expect("doc").node(id).is_none(), "one undo removes the section and its mark");
    }

    #[test]
    fn selected_sections_are_converted_not_copied() {
        let mut s = Session::new();
        let made = s.execute("node.create", &json!({"type": "section", "x": 0, "y": 0, "name": "Landing"})).expect("section");
        let id = NodeId(made["id"].as_u64().expect("id"));
        s.set_selection(vec![id]);
        let r = s.execute("smartSection.create", &json!({"mode": "document"})).expect("converted");
        assert_eq!(r["ids"], json!([id.0]));
        assert_eq!(mode_of(s.doc().expect("doc").node(id).expect("node")).as_deref(), Some("document"));
        assert_eq!(s.doc().expect("doc").node(id).expect("node").name, "Landing");
        s.execute("smartSection.clear", &json!({})).expect("cleared");
        assert_eq!(mode_of(s.doc().expect("doc").node(id).expect("node")), None);
    }

    #[test]
    fn setting_a_mode_needs_a_section_and_a_valid_name() {
        let mut s = Session::new();
        let frame = s.execute("node.create", &json!({"type": "frame"})).expect("frame");
        let fid = NodeId(frame["id"].as_u64().expect("id"));
        assert!(s.execute("smartSection.setMode", &json!({"ids": [fid.0], "mode": "web"})).is_err(), "a frame is not a section");
        let section = s.execute("smartSection.create", &json!({})).expect("section");
        let id = section["ids"][0].clone();
        for bad in [json!({"ids": [id], "mode": "../etc"}), json!({"ids": [id]}), json!({"ids": [id], "mode": "a b"})] {
            assert!(s.execute("smartSection.setMode", &bad).is_err(), "{bad}");
        }
        assert!(s.execute("smartSection.setMode", &json!({"ids": [id], "mode": "app"})).is_ok());
    }

    #[test]
    fn a_damaged_mark_reads_as_the_default_mode() {
        let mut s = Session::new();
        let r = s.execute("smartSection.create", &json!({})).expect("section");
        let id = NodeId(r["ids"][0].as_u64().expect("id"));
        for junk in ["", "not json", "{}", r#"{"mode": "../../x"}"#, r#"{"mode": 3}"#] {
            s.doc_mut().expect("doc").get_mut(id).expect("node").plugin_data.insert(KEY.into(), junk.into());
            assert_eq!(mode_of(s.doc().expect("doc").node(id).expect("node")).as_deref(), Some("web"), "{junk:?}");
        }
    }
}
