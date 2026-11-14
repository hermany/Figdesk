//! The AI tab's work folder, seen from the MCP server the agent runs. When `FIGDESK_WORK_DIR` names a
//! folder, every file path in a tool call must lie inside it: a relative path is made relative to it,
//! and `..` or any path that leaves it is refused. `generate_image` keeps its pictures in it.
//!
//! This is a guard on the arguments the agent sends, not a sandbox: the app does not confine the
//! commands it runs for an attached client (the engine's roots confine the whole app, the person's own
//! menus included). It checks the tools that take a `path` and, in `run_command`, `batch` and the other
//! tools that carry command parameters, any value under a key that names a path, a file or a folder.

use std::path::{Component, Path, PathBuf};

use serde_json::{Map, Value};

/// The environment variable the app sets for the agent's MCP server.
pub const ENV: &str = "FIGDESK_WORK_DIR";

/// Tools whose `path` (and `variables`, for `open_file`) is a file on disk.
const FILE_TOOLS: &[&str] = &["open_file", "save_file", "export"];
/// Tools that carry command parameters: their values are scanned for paths.
const COMMAND_TOOLS: &[&str] = &["run_command", "batch", "create_node", "set_properties"];

#[derive(Debug)]
pub struct WorkDir {
    root: PathBuf,
}

/// Whether `key` names a file, a folder or a path.
fn names_a_path(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    k.contains("path") || matches!(k.as_str(), "file" | "filename" | "dir" | "directory" | "folder" | "variables")
}

impl WorkDir {
    pub fn new(dir: &Path) -> Result<Self, String> {
        let root = dir.canonicalize().map_err(|e| format!("the work folder {} can't be used: {e}", dir.display()))?;
        if !root.is_dir() {
            return Err(format!("the work folder {} is not a folder", dir.display()));
        }
        Ok(Self { root })
    }

    /// The folder from the environment: none when the variable isn't set.
    pub fn from_env() -> Option<Result<Self, String>> {
        let value = std::env::var_os(ENV).filter(|v| !v.is_empty())?;
        Some(Self::new(Path::new(&value)))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// `path` as a file inside the folder (it need not exist yet): a relative path starts from the
    /// folder; `..`, `~` and every path that ends up outside are refused.
    pub fn resolve(&self, path: &str) -> Result<PathBuf, String> {
        let outside = || format!("`{path}` is outside the work folder {}: the AI may only use files inside it", self.root.display());
        if path.trim().is_empty() || path.starts_with('~') {
            return Err(outside());
        }
        let given = Path::new(path);
        if given.components().any(|c| matches!(c, Component::ParentDir)) {
            return Err(outside());
        }
        let joined = if given.is_absolute() { given.to_path_buf() } else { self.root.join(given) };
        // The nearest part that exists decides where the path really is (a link can lead out).
        let existing = joined.ancestors().find(|a| a.exists()).ok_or_else(outside)?;
        let rest = joined.strip_prefix(existing).map_err(|_| outside())?;
        let real = existing.canonicalize().map_err(|_| outside())?.join(rest);
        if real.starts_with(&self.root) { Ok(real) } else { Err(outside()) }
    }

    /// Check what a call to `tool` would touch and make its paths absolute inside the folder.
    pub fn guard(&self, tool: &str, args: &mut Map<String, Value>) -> Result<(), String> {
        if FILE_TOOLS.contains(&tool) {
            for key in ["path", "variables"] {
                if let Some(Value::String(p)) = args.get(key) {
                    let resolved = self.resolve(p)?;
                    args.insert(key.into(), Value::String(resolved.to_string_lossy().into_owned()));
                }
            }
        }
        if COMMAND_TOOLS.contains(&tool) {
            for value in args.values_mut() {
                self.scan(value)?;
            }
        }
        Ok(())
    }

    /// Walk command parameters; a string under a key that names a path is resolved in place.
    fn scan(&self, value: &mut Value) -> Result<(), String> {
        match value {
            Value::Object(map) => {
                for (key, v) in map.iter_mut() {
                    match v {
                        Value::String(p) if names_a_path(key) => *p = self.resolve(p)?.to_string_lossy().into_owned(),
                        Value::Array(items) if names_a_path(key) => {
                            for item in items.iter_mut() {
                                if let Value::String(p) = item {
                                    *p = self.resolve(p)?.to_string_lossy().into_owned();
                                }
                            }
                        }
                        other => self.scan(other)?,
                    }
                }
            }
            Value::Array(items) => {
                for item in items {
                    self.scan(item)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}

/// The folder this process was given, read once.
pub fn current() -> Option<&'static Result<WorkDir, String>> {
    static CURRENT: std::sync::LazyLock<Option<Result<WorkDir, String>>> = std::sync::LazyLock::new(WorkDir::from_env);
    CURRENT.as_ref()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn folder(name: &str) -> (PathBuf, WorkDir) {
        let dir = std::env::temp_dir().join(format!("figdesk-workdir-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create");
        let work = WorkDir::new(&dir).expect("work dir");
        (dir, work)
    }

    #[test]
    fn relative_paths_start_from_the_folder_and_absolute_ones_inside_it_pass() {
        let (dir, work) = folder("a");
        assert_eq!(work.resolve("out/pic.png").expect("inside"), work.root().join("out/pic.png"));
        assert_eq!(work.resolve(work.root().join("x.png").to_str().expect("utf8")).expect("inside"), work.root().join("x.png"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn paths_that_leave_the_folder_are_refused() {
        let (dir, work) = folder("b");
        for bad in ["../x.png", "a/../../x.png", "/etc/passwd", "~/x", "", "  "] {
            assert!(work.resolve(bad).is_err(), "{bad:?}");
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_link_that_leads_out_is_refused() {
        let (dir, work) = folder("c");
        let outside = std::env::temp_dir().join(format!("figdesk-workdir-out-{}", std::process::id()));
        std::fs::create_dir_all(&outside).expect("create");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside, dir.join("link")).expect("link");
            assert!(work.resolve("link/x.png").is_err());
        }
        let _ = std::fs::remove_dir_all(dir);
        let _ = std::fs::remove_dir_all(outside);
    }

    #[test]
    fn file_tools_get_absolute_paths_in_the_folder() {
        let (dir, work) = folder("d");
        let mut args = json!({"path": "board.png"}).as_object().cloned().expect("object");
        work.guard("export", &mut args).expect("inside");
        assert_eq!(args["path"], work.root().join("board.png").to_string_lossy().as_ref());
        let mut bad = json!({"path": "/tmp/x.png"}).as_object().cloned().expect("object");
        assert!(work.guard("save_file", &mut bad).is_err());
        let mut other = json!({"path": "/tmp/x.png"}).as_object().cloned().expect("object");
        assert!(work.guard("screenshot", &mut other).is_ok(), "tools without files are left alone");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn command_parameters_are_scanned_for_paths_at_any_depth() {
        let (dir, work) = folder("e");
        let mut ok = json!({"command": "image.place", "params": {"path": "a.png", "x": 1}}).as_object().cloned().expect("object");
        work.guard("run_command", &mut ok).expect("inside");
        assert_eq!(ok["params"]["path"], work.root().join("a.png").to_string_lossy().as_ref());
        let mut batch = json!({"commands": [{"command": "x", "params": {"ok": 1}}, {"command": "file.save", "params": {"path": "/etc/x"}}]})
            .as_object()
            .cloned()
            .expect("object");
        assert!(work.guard("batch", &mut batch).is_err());
        let mut many = json!({"command": "x", "params": {"paths": ["a.png", "../b.png"]}}).as_object().cloned().expect("object");
        assert!(work.guard("run_command", &mut many).is_err());
        let mut text = json!({"command": "board.sticky", "params": {"text": "/api/users is a path in a note"}}).as_object().cloned().expect("object");
        assert!(work.guard("run_command", &mut text).is_ok(), "text that looks like a path is not a path");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_missing_or_file_folder_is_an_error() {
        assert!(WorkDir::new(Path::new("/definitely/not/here")).is_err());
        let file = std::env::temp_dir().join(format!("figdesk-workdir-file-{}", std::process::id()));
        std::fs::write(&file, "x").expect("write");
        assert!(WorkDir::new(&file).is_err());
        let _ = std::fs::remove_file(file);
    }
}
