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

/// `name` as an executable: on `PATH` or in the folders command-line tools usually install to (an
/// app opened from the Finder has a short `PATH`).
fn find_program(name: &str) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        dirs.extend([".local/bin", ".npm-global/bin", ".claude/local", ".bun/bin", ".volta/bin"].map(|d| home.join(d)));
    }
    dirs.extend(["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"].map(PathBuf::from));
    dirs.into_iter().map(|d| d.join(name)).find(|p| p.is_file())
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

fn command(program: &Path, cli: &Path, port: u16, prompt: &str, session: Option<&str>, model: Option<&str>) -> Command {
    let mcp = json!({"mcpServers": {MCP_NAME: {"command": cli, "args": ["mcp", "--attach", port.to_string()]}}});
    let mut c = Command::new(program);
    c.args(["-p", "--output-format", "stream-json", "--verbose", "--include-partial-messages"])
        .args(["--mcp-config", &mcp.to_string(), "--strict-mcp-config"])
        .args(["--tools", ""])
        .args(["--allowedTools", &format!("mcp__{MCP_NAME}")])
        .args(["--disallowedTools"])
        .args(["open_file", "save_file", "export"].map(|t| format!("mcp__{MCP_NAME}__{t}")))
        .args(["--append-system-prompt", SYSTEM_PROMPT]);
    if let Some(id) = session {
        c.args(["--resume", id]);
    }
    if let Some(model) = model {
        c.args(["--model", model]);
    }
    // After `--`, so a prompt that starts with a dash is still a prompt.
    c.args(["--", prompt]);
    // Not in the user's project: the agent works on the design only.
    c.current_dir(std::env::temp_dir()).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
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
}

impl Agent {
    pub fn new(ctx: Shared<Option<egui::Context>>) -> Self {
        Self { child: Arc::default(), events: Arc::default(), ctx }
    }

    /// Start a turn: `prompt` to the agent, attached to the control channel on `port`.
    pub fn send(&self, prompt: &str, session: Option<&str>, port: u16, model: Option<&str>) -> Result<(), String> {
        if lock(&self.child).is_some() {
            return Err("the agent is still answering".into());
        }
        let program = find_program("claude").ok_or("Claude Code (the `claude` command) was not found. Install it and sign in, then try again.")?;
        let cli = cli_beside_exe().ok_or("figcraft-cli was not found next to the app: the agent has no way to reach the design.")?;
        let mut child = command(&program, &cli, port, prompt, session, model).spawn().map_err(|e| format!("could not start claude: {e}"))?;
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
                        let parsed = parse_line(&String::from_utf8_lossy(&line));
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
                        (Some(s), true) => format!("claude ended without an answer ({s})"),
                        (None, true) => "claude ended without an answer".into(),
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

    #[test]
    fn the_command_attaches_to_the_port_with_design_tools_only() {
        let c = command(Path::new("/bin/claude"), Path::new("/app/figcraft-cli"), 7979, "draw", Some("sid"), Some("opus"));
        let args: Vec<String> = c.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        let after = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
        assert_eq!(after("--").as_deref(), Some("draw"));
        assert_eq!(after("--resume").as_deref(), Some("sid"));
        assert_eq!(after("--model").as_deref(), Some("opus"));
        assert_eq!(after("--tools").as_deref(), Some(""));
        assert_eq!(after("--allowedTools").as_deref(), Some("mcp__figcraft"));
        let mcp: Value = serde_json::from_str(&after("--mcp-config").unwrap_or_default()).unwrap_or_default();
        assert_eq!(mcp["mcpServers"]["figcraft"]["args"], json!(["mcp", "--attach", "7979"]));
        for tool in ["open_file", "save_file", "export"] {
            assert!(args.contains(&format!("mcp__figcraft__{tool}")), "{tool} is refused");
        }
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
