//! `generate_image`: make a picture with OpenAI's image API and place it in the open document.
//!
//! The request goes through the system's `curl`, with its options (the key included) given on its
//! standard input, so the key never shows in a process list and the workspace takes no HTTP and TLS
//! stack. The key comes from the environment (`OPENAI_API_KEY`) and is never written anywhere. The
//! picture comes back as base64 and is placed with `image.place {data}`.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde_json::{Map, Value, json};

use crate::backend::Backend;
use crate::tools::ToolResult;
use crate::workdir::WorkDir;

const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
const DEFAULT_MODEL: &str = "gpt-image-1";
/// Longest the request may take, in seconds: image generation is slow.
const MAX_SECONDS: u32 = 240;
/// Most of the reply read, in bytes: a large picture comes back as base64.
const MAX_REPLY: u64 = 96 * 1024 * 1024;
/// Longest prompt sent, in characters.
const MAX_PROMPT_CHARS: usize = 4000;
/// The sizes the API takes.
pub const SIZES: &[&str] = &["auto", "1024x1024", "1536x1024", "1024x1536"];

/// Where the images come from.
pub struct ImageApi {
    pub base_url: String,
    pub key: String,
    pub model: String,
}

impl ImageApi {
    /// The API as the environment describes it: `OPENAI_API_KEY` (required), `OPENAI_BASE_URL` and
    /// `FIGDESK_IMAGE_MODEL`.
    pub fn from_env() -> Result<Self, String> {
        let key = std::env::var("OPENAI_API_KEY").ok().filter(|k| !k.trim().is_empty()).ok_or(
            "OPENAI_API_KEY is not set: images are made with OpenAI's image API, which needs an API key (a ChatGPT subscription does not include one). \
             Set it in the environment the app starts from.",
        )?;
        let base_url = std::env::var("OPENAI_BASE_URL").ok().filter(|u| !u.trim().is_empty()).unwrap_or_else(|| DEFAULT_BASE_URL.into());
        let model = std::env::var("FIGDESK_IMAGE_MODEL").ok().filter(|m| !m.trim().is_empty()).unwrap_or_else(|| DEFAULT_MODEL.into());
        Ok(Self { base_url, key: key.trim().to_string(), model })
    }
}

/// `s` as a quoted string of curl's option files, which take backslash escapes.
fn curl_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The request body: the prompt, the size and, when asked, a transparent background.
fn request_body(model: &str, prompt: &str, size: &str, transparent: bool) -> Value {
    let mut body = json!({"model": model, "prompt": prompt, "size": size, "n": 1, "output_format": "png"});
    if transparent && let Some(o) = body.as_object_mut() {
        o.insert("background".into(), json!("transparent"));
    }
    body
}

/// The base64 picture in the API's reply, or the reason it gave for refusing.
fn picture_in(reply: &str) -> Result<String, String> {
    let v: Value = serde_json::from_str(reply)
        .map_err(|_| format!("the image API sent something that is not JSON: {}", reply.chars().take(200).collect::<String>()))?;
    if let Some(message) = v.pointer("/error/message").and_then(Value::as_str) {
        return Err(format!("the image API refused the request: {message}"));
    }
    v.pointer("/data/0/b64_json")
        .and_then(Value::as_str)
        .filter(|d| !d.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "the image API's reply has no picture".into())
}

/// Ask the API for a picture; the base64 PNG.
pub fn request_image(api: &ImageApi, prompt: &str, size: &str, transparent: bool) -> Result<String, String> {
    let body = request_body(&api.model, prompt, size, transparent).to_string();
    let url = format!("{}/images/generations", api.base_url.trim_end_matches('/'));
    let config = format!(
        "url = {}\nrequest = \"POST\"\nsilent\nshow-error\nmax-time = {MAX_SECONDS}\nheader = {}\nheader = \"Content-Type: application/json\"\ndata = {}\n",
        curl_quote(&url),
        curl_quote(&format!("Authorization: Bearer {}", api.key)),
        curl_quote(&body)
    );
    let mut child = Command::new("curl")
        .args(["--config", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not run curl (the image request needs it): {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(config.as_bytes()).map_err(|e| format!("could not give curl its options: {e}"))?;
    }
    let mut reply = String::new();
    if let Some(out) = child.stdout.take() {
        let _ = out.take(MAX_REPLY).read_to_string(&mut reply);
    }
    let mut errors = String::new();
    if let Some(err) = child.stderr.take() {
        let _ = err.take(4096).read_to_string(&mut errors);
    }
    let status = child.wait().map_err(|e| format!("curl: {e}"))?;
    if !status.success() && reply.trim().is_empty() {
        return Err(format!("the image request failed: {}", errors.trim()));
    }
    picture_in(&reply)
}

/// `generate_image {prompt, size?, transparent?, name?, x?, y?, width?, height?, parent?}`: make the
/// picture, then place it.
pub fn generate_image(b: &mut dyn Backend, a: &Map<String, Value>) -> Result<ToolResult, String> {
    let work = match crate::workdir::current() {
        Some(Ok(w)) => Some(w),
        Some(Err(e)) => return Err(e.clone()),
        None => None,
    };
    generate_image_with(b, a, &ImageApi::from_env()?, work)
}

/// A file name for a picture of `prompt`: its words, lowercase and joined by dashes, then `stamp`.
fn picture_name(prompt: &str, stamp: u64) -> String {
    let words: String = prompt.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect();
    let slug = words.split('-').filter(|w| !w.is_empty()).collect::<Vec<_>>().join("-");
    let slug: String = slug.chars().take(40).collect();
    format!("{}-{stamp}.png", if slug.is_empty() { "image" } else { slug.trim_end_matches('-') })
}

/// Keep the picture in the work folder's `images` folder; where it went.
fn save_picture(work: &WorkDir, prompt: &str, base64_png: &str) -> Result<PathBuf, String> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD.decode(base64_png).map_err(|e| format!("the picture could not be decoded: {e}"))?;
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let folder = work.root().join("images");
    std::fs::create_dir_all(&folder).map_err(|e| format!("could not make {}: {e}", folder.display()))?;
    let path = work.resolve(&folder.join(picture_name(prompt, stamp)).to_string_lossy())?;
    std::fs::write(&path, bytes).map_err(|e| format!("could not save the picture to {}: {e}", path.display()))?;
    Ok(path)
}

pub fn generate_image_with(b: &mut dyn Backend, a: &Map<String, Value>, api: &ImageApi, work: Option<&WorkDir>) -> Result<ToolResult, String> {
    let prompt: String = a
        .get("prompt")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .ok_or("`prompt` is required: describe the picture")?
        .chars()
        .take(MAX_PROMPT_CHARS)
        .collect();
    let size = a.get("size").and_then(Value::as_str).unwrap_or("auto");
    if !SIZES.contains(&size) {
        return Err(format!("`size` must be one of {}", SIZES.join(", ")));
    }
    let transparent = a.get("transparent").and_then(Value::as_bool).unwrap_or(false);
    let data = request_image(api, &prompt, size, transparent)?;
    let saved = work.map(|w| save_picture(w, &prompt, &data)).transpose()?;
    let mut params = json!({"data": data, "name": a.get("name").and_then(Value::as_str).unwrap_or("Generated image")});
    for key in ["x", "y", "width", "height", "parent"] {
        if let (Some(v), Some(o)) = (a.get(key).filter(|v| !v.is_null()), params.as_object_mut()) {
            o.insert(key.into(), v.clone());
        }
    }
    let mut placed = b.call("engine.execute", json!({"command": "image.place", "params": params}))?;
    if let (Some(path), Some(o)) = (saved, placed.as_object_mut()) {
        o.insert("savedTo".into(), json!(path.to_string_lossy()));
    }
    Ok(ToolResult::json(&placed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;

    /// A server that answers one request with `body`, and gives back the request it got.
    fn serve_once(body: String) -> (String, std::thread::JoinHandle<String>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
        let url = format!("http://{}", listener.local_addr().expect("addr"));
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            let mut head = String::new();
            let mut length = 0;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                    break;
                }
                if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = v.trim().parse().unwrap_or(0);
                }
                let end = line == "\r\n";
                head.push_str(&line);
                if end {
                    break;
                }
            }
            let mut payload = vec![0; length];
            let _ = reader.read_exact(&mut payload);
            head.push_str(&String::from_utf8_lossy(&payload));
            let reply =
                format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            let _ = stream.write_all(reply.as_bytes());
            head
        });
        (url, handle)
    }

    fn curl_available() -> bool {
        Command::new("curl").arg("--version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
    }

    /// A 1×1 PNG.
    const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

    fn api(url: &str) -> ImageApi {
        ImageApi { base_url: url.into(), key: "sk-test-key".into(), model: "gpt-image-1".into() }
    }

    #[test]
    fn curl_options_are_quoted_and_control_characters_dropped() {
        assert_eq!(curl_quote("a\"b\\c\nd\u{7}"), "\"a\\\"b\\\\c\\nd\"");
    }

    #[test]
    fn the_body_asks_for_a_transparent_background_only_when_told() {
        assert!(request_body("m", "p", "auto", false).get("background").is_none());
        assert_eq!(request_body("m", "p", "auto", true)["background"], "transparent");
    }

    #[test]
    fn the_picture_is_read_from_the_reply_and_a_refusal_says_why() {
        assert_eq!(picture_in(r#"{"data":[{"b64_json":"QUJD"}]}"#).as_deref(), Ok("QUJD"));
        let refused = picture_in(r#"{"error":{"message":"Incorrect API key provided"}}"#).expect_err("a refusal");
        assert!(refused.contains("Incorrect API key"), "{refused}");
        for junk in ["", "no", "{}", r#"{"data":[]}"#, r#"{"data":[{"b64_json":""}]}"#] {
            assert!(picture_in(junk).is_err(), "{junk:?}");
        }
    }

    #[test]
    fn the_request_carries_the_key_and_prompt_and_returns_the_picture() {
        if !curl_available() {
            return;
        }
        let (url, server) = serve_once(json!({"data": [{"b64_json": PNG}]}).to_string());
        let got = request_image(&api(&url), "a red \"fox\"", "1024x1024", true).expect("a picture");
        assert_eq!(got, PNG);
        let seen = server.join().expect("server");
        assert!(seen.starts_with("POST /images/generations"), "{seen}");
        assert!(seen.to_ascii_lowercase().contains("authorization: bearer sk-test-key"), "{seen}");
        assert!(seen.contains("a red \\\"fox\\\"") && seen.contains("transparent"), "{seen}");
    }

    #[test]
    fn an_api_error_reaches_the_agent_as_a_message() {
        if !curl_available() {
            return;
        }
        let (url, server) = serve_once(json!({"error": {"message": "billing hard limit reached"}}).to_string());
        let error = request_image(&api(&url), "x", "auto", false).expect_err("an error");
        assert!(error.contains("billing hard limit"), "{error}");
        let _ = server.join();
    }

    #[test]
    fn the_picture_is_placed_in_the_document() {
        if !curl_available() {
            return;
        }
        let (url, server) = serve_once(json!({"data": [{"b64_json": PNG}]}).to_string());
        let mut backend = crate::headless::Headless::new();
        let args = json!({"prompt": "a fox", "name": "Fox", "width": 64, "height": 64}).as_object().cloned().unwrap_or_default();
        let result = generate_image_with(&mut backend, &args, &api(&url), None).expect("placed");
        let _ = server.join();
        assert!(!result.is_error, "{}", result.first_text());
        let placed: Value = serde_json::from_str(result.first_text()).expect("json");
        assert!(placed.get("id").is_some(), "{placed}");
    }

    #[test]
    fn pictures_get_a_readable_name_from_their_prompt() {
        assert_eq!(picture_name("A red  fox, in the snow!", 7), "a-red-fox-in-the-snow-7.png");
        assert_eq!(picture_name("¡¿?!", 7), "image-7.png");
        assert!(picture_name(&"palabra ".repeat(20), 1).len() <= 40 + 12);
    }

    #[test]
    fn with_a_work_folder_the_picture_is_kept_there_and_placed() {
        if !curl_available() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("figdesk-images-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create");
        let work = WorkDir::new(&dir).expect("work dir");
        let (url, server) = serve_once(json!({"data": [{"b64_json": PNG}]}).to_string());
        let mut backend = crate::headless::Headless::new();
        let args = json!({"prompt": "a fox"}).as_object().cloned().unwrap_or_default();
        let result = generate_image_with(&mut backend, &args, &api(&url), Some(&work)).expect("placed");
        let _ = server.join();
        let placed: Value = serde_json::from_str(result.first_text()).expect("json");
        let saved = PathBuf::from(placed["savedTo"].as_str().expect("savedTo"));
        assert!(saved.starts_with(work.root().join("images")) && saved.is_file(), "{saved:?}");
        assert!(placed.get("id").is_some(), "{placed}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_missing_prompt_or_a_wrong_size_is_refused_before_any_request() {
        let mut backend = crate::headless::Headless::new();
        let none = Map::new();
        assert!(generate_image_with(&mut backend, &none, &api("http://127.0.0.1:9"), None).is_err());
        let bad = json!({"prompt": "x", "size": "5x5"}).as_object().cloned().unwrap_or_default();
        assert!(generate_image_with(&mut backend, &bad, &api("http://127.0.0.1:9"), None).expect_err("size").contains("size"));
    }
}
