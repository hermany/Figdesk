//! The AI agent behind the AI tab's chat: the user's own `claude` command line, run once per prompt
//! (`claude -p`) with this app's MCP server attached, so it designs in the open document. It uses the
//! user's existing Claude login and keeps no key. The conversation continues across prompts through
//! the session id the command reports.
//!
//! The agent gets the design tools of the MCP server and nothing else (`--tools ""` removes its own
//! file, shell and web tools), and not the tools that open or save files. `run_command` can still
//! reach file commands: the app does not confine them, as `figcraft-cli mcp` does with its roots.

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

use figcraft_ui_egui::ai_chat::Provider;
use serde_json::{Value, json};

/// The longest line read from the agent, in bytes: a tool result can carry a screenshot.
const MAX_LINE: usize = 16 * 1024 * 1024;
/// How much of the agent's error output is kept to say why it failed.
const MAX_STDERR: usize = 4096;
const MCP_NAME: &str = "figcraft";

/// What the agent reports, in the order it happens.
#[derive(Debug, PartialEq, Eq)]
pub enum Event {
    /// The id to continue this conversation with.
    Session(String),
    /// A piece of the answer.
    Text(String),
    /// The name of a design tool it called.
    Tool(String),
    /// The turn ended; `Some` says why it failed.
    Done(Option<String>),
}

/// The events a line of the agent's `stream-json` output stands for.
fn parse_line(line: &str) -> Vec<Event> {
    let Ok(v) = serde_json::from_str::<Value>(line) else { return Vec::new() };
    let kind = v.get("type").and_then(Value::as_str).unwrap_or("");
    let mut out = Vec::new();
    match kind {
        "system" if v.get("subtype").and_then(Value::as_str) == Some("init") => {
            if let Some(id) = v.get("session_id").and_then(Value::as_str) {
                out.push(Event::Session(id.to_string()));
            }
        }
        // The answer arrives as deltas; the final `assistant` message repeats it, so only its tool calls count.
        "stream_event" => {
            let event = v.get("event");
            let delta = event.filter(|e| e.get("type").and_then(Value::as_str) == Some("content_block_delta")).and_then(|e| e.get("delta"));
            if let Some(text) =
                delta.filter(|d| d.get("type").and_then(Value::as_str) == Some("text_delta")).and_then(|d| d.get("text")).and_then(Value::as_str)
            {
                out.push(Event::Text(text.to_string()));
            }
        }
        "assistant" => {
            let blocks = v.pointer("/message/content").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]);
            for block in blocks.iter().filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_use")) {
                if let Some(name) = block.get("name").and_then(Value::as_str) {
                    let prefix = format!("mcp__{MCP_NAME}__");
                    out.push(Event::Tool(name.strip_prefix(prefix.as_str()).unwrap_or(name).to_string()));
                }
            }
        }
        "result" => {
            let failed = v.get("is_error").and_then(Value::as_bool).unwrap_or(false);
            let why = v.get("result").and_then(Value::as_str).unwrap_or("the agent stopped with an error");
            out.push(Event::Done(failed.then(|| why.to_string())));
        }
        _ => {}
    }
    out
}

/// The events a line of `codex exec --json` stands for. Codex reports whole messages, not pieces of
/// them, and each tool call once when it starts.
fn parse_codex_line(line: &str) -> Vec<Event> {
    let Ok(v) = serde_json::from_str::<Value>(line) else { return Vec::new() };
    let kind = v.get("type").and_then(Value::as_str).unwrap_or("");
    let item = v.get("item");
    let item_kind = item.and_then(|i| i.get("type")).and_then(Value::as_str).unwrap_or("");
    match (kind, item_kind) {
        ("thread.started", _) => v.get("thread_id").and_then(Value::as_str).map(|id| vec![Event::Session(id.to_string())]).unwrap_or_default(),
        ("item.completed", "agent_message") => {
            let text = item.and_then(|i| i.get("text")).and_then(Value::as_str).unwrap_or("");
            if text.is_empty() { Vec::new() } else { vec![Event::Text(text.to_string())] }
        }
        ("item.started", "mcp_tool_call") => {
            item.and_then(|i| i.get("tool")).and_then(Value::as_str).map(|t| vec![Event::Tool(t.to_string())]).unwrap_or_default()
        }
        // The shell is read-only and the app never asks for it, but a read still shows.
        ("item.started", "command_execution") => vec![Event::Tool("shell".into())],
        ("turn.completed", _) => vec![Event::Done(None)],
        ("turn.failed", _) => {
            let why = v.pointer("/error/message").and_then(Value::as_str).unwrap_or("codex stopped with an error");
            vec![Event::Done(Some(why.to_string()))]
        }
        _ => Vec::new(),
    }
}

/// What the user's login shell says `name` is, asked as an interactive login shell (where `.zprofile`
/// and `.zshrc` set up `PATH`, version managers and keys). An app opened from the Finder has none of
/// that. Gives up after a few seconds.
fn shell_env(name: &str) -> Option<String> {
    let shell = std::env::var("SHELL").ok().filter(|s| Path::new(s).is_file())?;
    let mut child = Command::new(shell)
        .args(["-ilc", &format!("printenv {name}")])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if std::time::Instant::now() < deadline => std::thread::sleep(std::time::Duration::from_millis(40)),
            _ => {
                let _ = child.kill();
                return None;
            }
        }
    };
    let mut out = String::new();
    child.stdout.take()?.take(64 * 1024).read_to_string(&mut out).ok()?;
    // Shell start-up files can print before the value: it is the last line.
    let value = out.lines().rev().map(str::trim).find(|l| !l.is_empty())?.to_string();
    status.success().then_some(value)
}

/// The folders the agent's own tools are looked for in, and the `PATH` it runs with: the program's own
/// folder, the login shell's `PATH`, this process's, and the places command-line tools install to.
fn search_dirs(program_dir: Option<&Path>, shell_path: Option<&str>, current: Option<&std::ffi::OsStr>, home: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = program_dir.map(Path::to_path_buf).into_iter().collect();
    dirs.extend(shell_path.map(|p| std::env::split_paths(p).collect::<Vec<_>>()).unwrap_or_default());
    dirs.extend(current.map(|p| std::env::split_paths(p).collect::<Vec<_>>()).unwrap_or_default());
    if let Some(home) = home {
        dirs.extend([".local/bin", ".npm-global/bin", ".claude/local", ".bun/bin", ".volta/bin"].map(|d| home.join(d)));
    }
    dirs.extend(["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"].map(PathBuf::from));
    let mut seen = std::collections::HashSet::new();
    dirs.retain(|d| !d.as_os_str().is_empty() && seen.insert(d.clone()));
    dirs
}

fn find_in(dirs: &[PathBuf], name: &str) -> Option<PathBuf> {
    dirs.iter().map(|d| d.join(name)).find(|p| p.is_file())
}

/// The `figcraft-cli` that sits beside this executable, which the agent runs as its MCP server.
fn cli_beside_exe() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let cli = exe.parent()?.join(if cfg!(windows) { "figcraft-cli.exe" } else { "figcraft-cli" });
    cli.is_file().then_some(cli)
}

const SYSTEM_PROMPT: &str = "You are working inside Figdesk, a design editor, through its MCP tools. \
Make the changes the user asks for directly in the open document, in small steps, and look at the result with the screenshot tool. \
Then say in a sentence or two what you did. Reply in the user's language. Work only with the design: do not look for files.";

/// What the agent runs with: which provider, and what the call needs to continue a conversation or
/// reach the image API.
struct Call<'a> {
    cli: &'a Path,
    port: u16,
    prompt: &'a str,
    session: Option<&'a str>,
    model: Option<&'a str>,
    /// The key is in the environment of the command (it never goes on the command line).
    has_key: bool,
}

/// Where the agent works from: not the user's project, only the design.
fn work_dir() -> PathBuf {
    std::env::temp_dir()
}

fn claude_command(program: &Path, call: &Call) -> Command {
    let mut server = json!({"command": call.cli, "args": ["mcp", "--attach", call.port.to_string()]});
    if call.has_key
        && let Some(o) = server.as_object_mut()
    {
        o.insert("env".into(), json!({"OPENAI_API_KEY": "${OPENAI_API_KEY}"}));
    }
    let mcp = json!({"mcpServers": {MCP_NAME: server}});
    let mut c = Command::new(program);
    c.args(["-p", "--output-format", "stream-json", "--verbose", "--include-partial-messages"])
        .args(["--mcp-config", &mcp.to_string(), "--strict-mcp-config"])
        .args(["--tools", ""])
        .args(["--allowedTools", &format!("mcp__{MCP_NAME}")])
        .args(["--disallowedTools"])
        .args(["open_file", "save_file", "export"].map(|t| format!("mcp__{MCP_NAME}__{t}")))
        .args(["--append-system-prompt", SYSTEM_PROMPT]);
    if let Some(id) = call.session {
        c.args(["--resume", id]);
    }
    if let Some(model) = call.model {
        c.args(["--model", model]);
    }
    // After `--`, so a prompt that starts with a dash is still a prompt.
    c.args(["--", call.prompt]);
    c.current_dir(work_dir()).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    c
}

/// The `model` of the user's own Codex configuration (`--ignore-user-config` leaves it out of the run).
fn codex_model() -> Option<String> {
    let home = std::env::var_os("CODEX_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".codex")))?;
    let text = std::fs::read_to_string(home.join("config.toml")).ok()?;
    // Only the top of the file: later tables have keys of their own called `model`.
    text.lines().take_while(|l| !l.trim_start().starts_with('[')).find_map(|l| {
        let (key, value) = l.split_once('=')?;
        (key.trim() == "model").then(|| value.trim().trim_matches('"').to_string()).filter(|m| !m.is_empty())
    })
}

/// `codex exec` run on its own: the user's configuration (and with it their plugins, skills and
/// other tools, Computer Use among them) is not loaded, only their login, and this app's MCP server.
fn codex_command(program: &Path, call: &Call, user_model: Option<&str>) -> Command {
    let mut c = Command::new(program);
    c.arg("exec");
    if let Some(id) = call.session {
        c.args(["resume", id]);
    }
    c.args(["--json", "--skip-git-repo-check", "--ignore-user-config", "--ignore-rules"]);
    c.args(["-c", "sandbox_mode=\"read-only\""]);
    if let Some(model) = user_model {
        c.args(["-m", model]);
    }
    let q = |v: Value| v.to_string();
    c.args(["-c", &format!("mcp_servers.{MCP_NAME}.command={}", q(json!(call.cli)))]);
    c.args(["-c", &format!("mcp_servers.{MCP_NAME}.args={}", q(json!(["mcp", "--attach", call.port.to_string()])))]);
    c.args(["-c", &format!("mcp_servers.{MCP_NAME}.disabled_tools={}", q(json!(["open_file", "save_file", "export"])))]);
    if call.has_key {
        c.args(["-c", &format!("mcp_servers.{MCP_NAME}.env_vars={}", q(json!(["OPENAI_API_KEY"])))]);
    }
    c.args(["--", &format!("{SYSTEM_PROMPT}\n\n{}", call.prompt)]);
    c.current_dir(work_dir()).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    c
}

/// Read one line of at most [`MAX_LINE`] bytes into `buf`; false at the end of the stream, an error or a longer line.
fn read_bounded_line(reader: &mut impl BufRead, buf: &mut Vec<u8>) -> bool {
    buf.clear();
    match reader.take(MAX_LINE as u64 + 1).read_until(b'\n', buf) {
        Ok(0) | Err(_) => false,
        Ok(_) => buf.len() <= MAX_LINE,
    }
}

type Shared<T> = Arc<Mutex<T>>;

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The running agent turn, if there is one, and what it has reported.
#[derive(Default)]
pub struct Agent {
    child: Shared<Option<Child>>,
    events: Shared<Vec<Event>>,
    ctx: Shared<Option<egui::Context>>,
    /// What the user's login shell knows, looked for once in the background (it can take a moment):
    /// the OpenAI key for the image tool and the `PATH` the agent's tools are found by.
    shell: Shared<ShellEnv>,
}

#[derive(Default)]
struct ShellEnv {
    key: Option<String>,
    path: Option<String>,
}

impl Agent {
    pub fn new(ctx: Shared<Option<egui::Context>>) -> Self {
        let shell: Shared<ShellEnv> = Arc::default();
        let slot = shell.clone();
        let _ = std::thread::Builder::new().name("figcraft-agent-env".into()).spawn(move || {
            let key = std::env::var("OPENAI_API_KEY").ok().filter(|k| !k.trim().is_empty()).or_else(|| shell_env("OPENAI_API_KEY"));
            let path = shell_env("PATH");
            *lock(&slot) = ShellEnv { key, path };
        });
        Self { child: Arc::default(), events: Arc::default(), ctx, shell }
    }

    /// Start a turn: `prompt` to `provider`'s agent, attached to the control channel on `port`.
    pub fn send(&self, provider: Provider, prompt: &str, session: Option<&str>, port: u16, model: Option<&str>) -> Result<(), String> {
        if lock(&self.child).is_some() {
            return Err("the agent is still answering".into());
        }
        let (name, install) = match provider {
            Provider::Claude => ("claude", "Claude Code (the `claude` command) was not found. Install it and sign in, then try again."),
            Provider::Codex => ("codex", "Codex (the `codex` command) was not found. Install it and sign in, then try again."),
        };
        let (key, shell_path) = {
            let env = lock(&self.shell);
            (env.key.clone(), env.path.clone())
        };
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let dirs = search_dirs(None, shell_path.as_deref(), std::env::var_os("PATH").as_deref(), home.as_deref());
        let program = find_in(&dirs, name).ok_or(install)?;
        let cli = cli_beside_exe().ok_or("figcraft-cli was not found next to the app: the agent has no way to reach the design.")?;
        let call = Call { cli: &cli, port, prompt, session, model, has_key: key.is_some() };
        let mut cmd = match provider {
            Provider::Claude => claude_command(&program, &call),
            Provider::Codex => codex_command(&program, &call, codex_model().as_deref()),
        };
        if let Some(key) = &key {
            cmd.env("OPENAI_API_KEY", key);
        }
        // The agent's own start-up (its hooks, the tools they call) needs what a terminal would have.
        let run_dirs = search_dirs(program.parent(), shell_path.as_deref(), std::env::var_os("PATH").as_deref(), home.as_deref());
        if let Ok(path) = std::env::join_paths(&run_dirs) {
            cmd.env("PATH", path);
        }
        let parse: fn(&str) -> Vec<Event> = match provider {
            Provider::Claude => parse_line,
            Provider::Codex => parse_codex_line,
        };
        let mut child = cmd.spawn().map_err(|e| format!("could not start {name}: {e}"))?;
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        *lock(&self.child) = Some(child);
        let (events, ctx, slot) = (self.events.clone(), self.ctx.clone(), self.child.clone());
        let wake = move || {
            if let Some(ctx) = lock(&ctx).as_ref() {
                ctx.request_repaint();
            }
        };
        let errors: Shared<Vec<u8>> = Arc::default();
        if let Some(mut err) = stderr {
            let keep = errors.clone();
            let _ = std::thread::Builder::new().name("figcraft-agent-err".into()).spawn(move || {
                let mut bytes = Vec::new();
                let _ = (&mut err).take(MAX_STDERR as u64).read_to_end(&mut bytes);
                *lock(&keep) = bytes;
            });
        }
        std::thread::Builder::new()
            .name("figcraft-agent".into())
            .spawn(move || {
                let mut done = false;
                if let Some(out) = stdout {
                    let mut reader = BufReader::new(out);
                    let mut line = Vec::new();
                    while read_bounded_line(&mut reader, &mut line) {
                        let parsed = parse(&String::from_utf8_lossy(&line));
                        done |= parsed.iter().any(|e| matches!(e, Event::Done(_)));
                        if !parsed.is_empty() {
                            lock(&events).extend(parsed);
                            wake();
                        }
                    }
                }
                let status = lock(&slot).take().and_then(|mut c| c.wait().ok());
                if !done {
                    let said = String::from_utf8_lossy(&lock(&errors)).trim().to_string();
                    let why = match (status, said.is_empty()) {
                        (Some(s), true) => format!("{name} ended without an answer ({s})"),
                        (None, true) => format!("{name} ended without an answer"),
                        (_, false) => said,
                    };
                    lock(&events).push(Event::Done(Some(why)));
                    wake();
                }
            })
            .map_err(|e| format!("could not follow the agent: {e}"))?;
        Ok(())
    }

    /// Stop the turn in progress; the reader reports it ended.
    pub fn cancel(&self) {
        if let Some(child) = lock(&self.child).as_mut() {
            let _ = child.kill();
        }
    }

    pub fn drain(&self) -> Vec<Event> {
        std::mem::take(&mut *lock(&self.events))
    }
}

impl Drop for Agent {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_session_id_comes_from_the_init_line() {
        let line = r#"{"type":"system","subtype":"init","session_id":"abc-123","tools":[]}"#;
        assert_eq!(parse_line(line), [Event::Session("abc-123".into())]);
    }

    #[test]
    fn answer_text_comes_from_text_deltas_only() {
        let delta = r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"Hola"}}}"#;
        assert_eq!(parse_line(delta), [Event::Text("Hola".into())]);
        let start = r#"{"type":"stream_event","event":{"type":"message_start"}}"#;
        assert!(parse_line(start).is_empty());
        let whole = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Hola"}]}}"#;
        assert!(parse_line(whole).is_empty(), "the final message repeats the deltas");
    }

    #[test]
    fn tool_calls_show_the_tool_without_the_server_prefix() {
        let line = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"x"},{"type":"tool_use","name":"mcp__figcraft__create_node","input":{}}]}}"#;
        assert_eq!(parse_line(line), [Event::Tool("create_node".into())]);
    }

    #[test]
    fn a_result_ends_the_turn_and_an_error_result_says_why() {
        assert_eq!(parse_line(r#"{"type":"result","subtype":"success","is_error":false,"result":"ok"}"#), [Event::Done(None)]);
        assert_eq!(parse_line(r#"{"type":"result","is_error":true,"result":"not logged in"}"#), [Event::Done(Some("not logged in".into()))]);
    }

    #[test]
    fn junk_lines_say_nothing() {
        for line in ["", "not json", "{}", r#"{"type":7}"#, r#"{"type":"stream_event"}"#, r#"{"type":"assistant","message":5}"#] {
            assert!(parse_line(line).is_empty(), "{line:?}");
        }
    }

    fn call<'a>(session: Option<&'a str>, model: Option<&'a str>, has_key: bool) -> Call<'a> {
        Call { cli: Path::new("/app/figcraft-cli"), port: 7979, prompt: "draw", session, model, has_key }
    }

    fn args_of(c: &Command) -> Vec<String> {
        c.get_args().map(|a| a.to_string_lossy().into_owned()).collect()
    }

    #[test]
    fn the_claude_command_attaches_to_the_port_with_design_tools_only() {
        let args = args_of(&claude_command(Path::new("/bin/claude"), &call(Some("sid"), Some("opus"), false)));
        let after = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
        assert_eq!(after("--").as_deref(), Some("draw"));
        assert_eq!(after("--resume").as_deref(), Some("sid"));
        assert_eq!(after("--model").as_deref(), Some("opus"));
        assert_eq!(after("--tools").as_deref(), Some(""));
        assert_eq!(after("--allowedTools").as_deref(), Some("mcp__figcraft"));
        let mcp: Value = serde_json::from_str(&after("--mcp-config").unwrap_or_default()).unwrap_or_default();
        assert_eq!(mcp["mcpServers"]["figcraft"]["args"], json!(["mcp", "--attach", "7979"]));
        assert!(mcp["mcpServers"]["figcraft"].get("env").is_none(), "no key, no env entry");
        for tool in ["open_file", "save_file", "export"] {
            assert!(args.contains(&format!("mcp__figcraft__{tool}")), "{tool} is refused");
        }
    }

    #[test]
    fn the_key_is_named_to_the_agent_never_written_on_its_command_line() {
        let claude = args_of(&claude_command(Path::new("/bin/claude"), &call(None, None, true))).join(" ");
        assert!(claude.contains("${OPENAI_API_KEY}"), "{claude}");
        let codex = args_of(&codex_command(Path::new("/bin/codex"), &call(None, None, true), None)).join(" ");
        assert!(codex.contains("env_vars=[\"OPENAI_API_KEY\"]"), "{codex}");
        assert!(!codex.contains("sk-") && !claude.contains("sk-"));
    }

    #[test]
    fn the_codex_command_runs_alone_with_the_app_server_and_a_read_only_sandbox() {
        let args = args_of(&codex_command(Path::new("/bin/codex"), &call(None, None, false), Some("some-model")));
        assert_eq!(&args[..1], ["exec"]);
        for flag in ["--ignore-user-config", "--ignore-rules", "--json"] {
            assert!(args.iter().any(|a| a == flag), "{flag}");
        }
        assert!(args.contains(&"sandbox_mode=\"read-only\"".to_string()));
        assert!(args.windows(2).any(|w| w[0] == "-m" && w[1] == "some-model"));
        assert!(args.contains(&"mcp_servers.figcraft.args=[\"mcp\",\"--attach\",\"7979\"]".to_string()));
        assert!(args.contains(&"mcp_servers.figcraft.disabled_tools=[\"open_file\",\"save_file\",\"export\"]".to_string()));
        assert!(args.last().is_some_and(|p| p.ends_with("draw")));
        let resumed = args_of(&codex_command(Path::new("/bin/codex"), &call(Some("thread-1"), None, false), None));
        assert_eq!(&resumed[..3], ["exec", "resume", "thread-1"]);
    }

    #[test]
    fn codex_lines_become_the_same_events() {
        assert_eq!(parse_codex_line(r#"{"type":"thread.started","thread_id":"t-9"}"#), [Event::Session("t-9".into())]);
        assert_eq!(parse_codex_line(r#"{"type":"item.completed","item":{"type":"agent_message","text":"Hecho"}}"#), [Event::Text("Hecho".into())]);
        assert_eq!(
            parse_codex_line(r#"{"type":"item.started","item":{"type":"mcp_tool_call","server":"figcraft","tool":"create_node"}}"#),
            [Event::Tool("create_node".into())]
        );
        assert!(
            parse_codex_line(r#"{"type":"item.completed","item":{"type":"mcp_tool_call","tool":"create_node"}}"#).is_empty(),
            "a call shows once"
        );
        assert_eq!(parse_codex_line(r#"{"type":"turn.completed","usage":{}}"#), [Event::Done(None)]);
        assert_eq!(parse_codex_line(r#"{"type":"turn.failed","error":{"message":"no model"}}"#), [Event::Done(Some("no model".into()))]);
        for junk in ["", "x", "{}", r#"{"type":"item.completed"}"#, r#"{"type":"item.started","item":5}"#] {
            assert!(parse_codex_line(junk).is_empty(), "{junk:?}");
        }
    }

    #[test]
    fn the_agents_path_has_the_shells_folders_first_without_repeats() {
        let dirs = search_dirs(
            Some(Path::new("/opt/homebrew/bin")),
            Some("/n/bin:/opt/homebrew/bin"),
            Some(std::ffi::OsStr::new("/usr/bin:/n/bin")),
            Some(Path::new("/h")),
        );
        assert_eq!(dirs[0], Path::new("/opt/homebrew/bin"));
        assert_eq!(dirs[1], Path::new("/n/bin"));
        assert!(dirs.contains(&PathBuf::from("/h/.npm-global/bin")));
        let unique: std::collections::HashSet<_> = dirs.iter().collect();
        assert_eq!(unique.len(), dirs.len(), "no folder twice: {dirs:?}");
        assert!(
            search_dirs(None, None, None, None).contains(&PathBuf::from("/usr/local/bin")),
            "the usual places are searched even with nothing known"
        );
    }

    #[test]
    fn a_program_is_found_in_the_first_folder_that_has_it() {
        let root = std::env::temp_dir().join(format!("figdesk-find-{}", std::process::id()));
        let (a, b) = (root.join("a"), root.join("b"));
        let _ = std::fs::create_dir_all(&a).and_then(|()| std::fs::create_dir_all(&b));
        let _ = std::fs::write(b.join("tool"), "x");
        assert_eq!(find_in(&[a.clone(), b.clone()], "tool"), Some(b.join("tool")));
        assert_eq!(find_in(&[a], "tool"), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_line_over_the_limit_stops_the_reader() {
        let mut long = vec![b'x'; MAX_LINE + 10];
        long.push(b'\n');
        let mut buf = Vec::new();
        assert!(!read_bounded_line(&mut std::io::Cursor::new(long), &mut buf));
        let mut ok = std::io::Cursor::new(b"{}\nnext\n".to_vec());
        assert!(read_bounded_line(&mut ok, &mut buf) && buf == b"{}\n");
    }
}
