//! The AI tab of the left sidebar: how to connect an AI client (Claude Code, Codex or any other MCP
//! client) to the open app. The MCP server is `figcraft-cli mcp --attach <port>`, which drives this
//! app through its control channel (docs/mcp.md); the tab shows whether that channel is listening and
//! gives each client the configuration to paste. Display only: it starts and stops nothing.

use egui::{Align2, CornerRadius, FontId, Rect, Sense, Stroke, Ui, pos2, vec2};

use crate::FigApp;
use crate::i18n::{tr, tr_noop};
use crate::theme::{Tokens, regular, semibold};

/// The port the docs and the AI panel use for `--control`.
pub const DEFAULT_PORT: u16 = 7979;

/// The clients the tab has a snippet for, in the order of its picker.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Client {
    #[default]
    ClaudeCode,
    Codex,
    Other,
}

impl Client {
    const ALL: [Client; 3] = [Client::ClaudeCode, Client::Codex, Client::Other];

    fn label(self) -> &'static str {
        match self {
            Client::ClaudeCode => tr_noop("Claude"),
            Client::Codex => tr_noop("Codex"),
            Client::Other => tr_noop("Other"),
        }
    }
}

/// What the switch asks of the app, which owns the control server (`apps/figcraft`): it carries the
/// request out on its next frame and reports the port, or the failure, back in `FigApp::ui`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Start,
    Stop,
}

/// What the tab remembers between frames.
#[derive(Clone, Copy, Debug, Default)]
pub struct AiState {
    pub client: Client,
    /// The text of the copy button turns to "Copied" for a moment after a click: the app clock time.
    pub copied_at: Option<f64>,
}

/// The `figcraft-cli` that sits beside this executable, else the bare name (found on `PATH`).
fn cli_path() -> String {
    let beside = std::env::current_exe().ok().and_then(|exe| exe.parent().map(|dir| dir.join("figcraft-cli")));
    match beside {
        Some(path) if path.is_file() => path.to_string_lossy().into_owned(),
        _ => "figcraft-cli".into(),
    }
}

/// `s` as one shell word: single quotes around anything but plain path characters.
fn shell_word(s: &str) -> String {
    if !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "/._-:@%+=".contains(c)) {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

/// `s` as a JSON or TOML basic string (the two agree on what these need escaped).
fn quoted(s: &str) -> String {
    serde_json::Value::String(s.to_string()).to_string()
}

/// The configuration that connects `client` to the app listening on `port`, with the CLI at `cli`.
pub fn snippet(client: Client, cli: &str, port: u16) -> String {
    match client {
        Client::ClaudeCode => format!("claude mcp add figcraft -- {} mcp --attach {port}", shell_word(cli)),
        Client::Codex => {
            format!("[mcp_servers.figcraft]\ncommand = {}\nargs = [\"mcp\", \"--attach\", \"{port}\"]", quoted(cli))
        }
        Client::Other => {
            format!(
                "{{\n  \"mcpServers\": {{\n    \"figcraft\": {{\n      \"command\": {},\n      \"args\": [\"mcp\", \"--attach\", \"{port}\"]\n    }}\n  }}\n}}",
                quoted(cli)
            )
        }
    }
}

/// Where the snippet is pasted, in words.
fn where_to(client: Client) -> &'static str {
    match client {
        Client::ClaudeCode => tr_noop("Run this in a terminal."),
        Client::Codex => tr_noop("Add this to ~/.codex/config.toml."),
        Client::Other => tr_noop("Add this to the MCP settings of your client."),
    }
}

pub fn show(app: &mut FigApp, ui: &mut Ui) {
    if app.ui.control_port.is_some() && !app.ui.chat.setup {
        crate::ai_chat::show(app, ui);
        return;
    }
    let pad = 16.0;
    let w = (ui.available_width() - 2.0 * pad).max(0.0);
    ui.add_space(12.0);
    egui::ScrollArea::vertical().id_salt("ai_tab").auto_shrink([false, false]).show(ui, |ui| {
        ui.set_max_width(w + 2.0 * pad);
        ui.horizontal(|ui| {
            ui.add_space(pad);
            ui.vertical(|ui| {
                ui.set_width(w);
                status(app, ui, w);
                ui.add_space(14.0);
                connect(app, ui, w);
            });
        });
    });
}

/// The line that says whether the control channel the MCP server attaches to is listening.
fn status(app: &mut FigApp, ui: &mut Ui, w: f32) {
    let t = Tokens::get(ui.ctx());
    let (text, on) = match app.ui.control_port {
        Some(port) => (format!("{} 127.0.0.1:{port}", tr("Listening on")), true),
        None => (tr("Not listening").to_string(), false),
    };
    let (r, _) = ui.allocate_exact_size(vec2(w, 20.0), Sense::hover());
    let dot = if on { t.accent } else { t.text_tertiary };
    ui.painter().circle_filled(pos2(r.min.x + 5.0, r.center().y), 4.0, dot);
    ui.painter().text(pos2(r.min.x + 16.0, r.center().y), Align2::LEFT_CENTER, text, semibold(12.0), t.text);
    ui.add_space(6.0);
    let label = if on { tr("Turn off") } else { tr("Turn on") };
    if text_button(ui, label, w.min(96.0)) {
        app.ui.control_action = Some(if on { Action::Stop } else { Action::Start });
        app.ui.chat.setup = false;
        app.ui.control_error = None;
    }
    if on && text_button(ui, tr("Back to chat"), w.min(120.0)) {
        app.ui.chat.setup = false;
    }
    if let Some(error) = app.ui.control_error.clone() {
        ui.add_space(6.0);
        wrapped(ui, &error, w, regular(11.0), t.text_secondary);
    }
}

fn connect(app: &mut FigApp, ui: &mut Ui, w: f32) {
    let t = Tokens::get(ui.ctx());
    let heading = tr("Connect your AI");
    wrapped(ui, heading, w, semibold(13.0), t.text);
    ui.add_space(8.0);
    let labels: Vec<&str> = Client::ALL.iter().map(|c| tr(c.label())).collect();
    let active = Client::ALL.iter().position(|c| *c == app.ui.ai.client).unwrap_or(0);
    if let Some(i) = crate::widgets::segmented_text(ui, &labels, active) {
        app.ui.ai.client = Client::ALL[i];
    }
    ui.add_space(8.0);
    let client = app.ui.ai.client;
    wrapped(ui, tr(where_to(client)), w, regular(11.0), t.text_secondary);
    ui.add_space(6.0);
    let port = app.ui.control_port.unwrap_or(DEFAULT_PORT);
    let text = snippet(client, &cli_path(), port);
    code_box(ui, &text, w);
    ui.add_space(8.0);
    let now = ui.input(|i| i.time);
    let recent = app.ui.ai.copied_at.is_some_and(|at| now - at < 1.5);
    if recent {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
    }
    let label = if recent { tr("Copied") } else { tr("Copy") };
    if text_button(ui, label, w.min(96.0)) {
        crate::egui_copy_text(ui.ctx(), &text);
        app.ui.ai.copied_at = Some(now);
    }
    ui.add_space(14.0);
    wrapped(
        ui,
        tr("The agent reaches this app only through 127.0.0.1. Edits show live, and undo works as usual."),
        w,
        regular(11.0),
        t.text_secondary,
    );
}

/// Text wrapped to `w`, as one block.
pub(crate) fn wrapped(ui: &mut Ui, text: &str, w: f32, font: FontId, color: egui::Color32) {
    let galley = ui.painter().layout(text.to_string(), font, color, w);
    let (r, _) = ui.allocate_exact_size(galley.size(), Sense::hover());
    ui.painter().galley(r.min, galley, color);
}

/// `text` in a monospace box, wrapped to `w`.
fn code_box(ui: &mut Ui, text: &str, w: f32) {
    let t = Tokens::get(ui.ctx());
    let inner = (w - 16.0).max(0.0);
    let galley = ui.painter().layout(text.to_string(), FontId::monospace(11.0), t.text, inner);
    let (r, _) = ui.allocate_exact_size(vec2(w, galley.size().y + 16.0), Sense::hover());
    ui.painter().rect(r, CornerRadius::same(6), t.field, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
    ui.painter().galley(pos2(r.min.x + 8.0, r.min.y + 8.0), galley, t.text);
}

/// A plain text button `w` wide; true when clicked.
pub(crate) fn text_button(ui: &mut Ui, label: &str, w: f32) -> bool {
    let t = Tokens::get(ui.ctx());
    let (r, _) = ui.allocate_exact_size(vec2(w, 28.0), Sense::hover());
    let resp = ui.interact(r, ui.scope_id().with(("ai_button", label)), Sense::click());
    crate::a11y::button(&resp, label);
    let fill = if resp.is_pointer_button_down_on() {
        t.pressed
    } else if resp.hovered() {
        t.hover
    } else {
        t.field
    };
    ui.painter().rect(Rect::from_min_size(r.min, r.size()), CornerRadius::same(6), fill, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
    ui.painter().text(r.center(), Align2::CENTER_CENTER, label, regular(12.0), t.text);
    resp.clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_code_command_names_the_cli_and_port() {
        assert_eq!(snippet(Client::ClaudeCode, "figcraft-cli", 7979), "claude mcp add figcraft -- figcraft-cli mcp --attach 7979");
    }

    #[test]
    fn paths_with_spaces_or_quotes_stay_one_shell_word() {
        let s = snippet(Client::ClaudeCode, "/Apps/Fig Craft/it's/figcraft-cli", 1);
        assert!(s.contains("'/Apps/Fig Craft/it'\\''s/figcraft-cli'"), "{s}");
    }

    #[test]
    fn codex_snippet_is_a_toml_table_with_the_attach_args() {
        let s = snippet(Client::Codex, "/x/figcraft-cli", 4242);
        assert_eq!(s, "[mcp_servers.figcraft]\ncommand = \"/x/figcraft-cli\"\nargs = [\"mcp\", \"--attach\", \"4242\"]");
    }

    #[test]
    fn generic_snippet_is_valid_json_with_the_server_entry() {
        let s = snippet(Client::Other, "C:\\Program Files\\figcraft-cli.exe", 7979);
        let v: serde_json::Value = serde_json::from_str(&s).expect("valid JSON");
        let server = &v["mcpServers"]["figcraft"];
        assert_eq!(server["command"], "C:\\Program Files\\figcraft-cli.exe");
        assert_eq!(server["args"], serde_json::json!(["mcp", "--attach", "7979"]));
    }
}
