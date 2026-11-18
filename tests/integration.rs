use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

fn temp_vault() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("omr-test-{}-{}", std::process::id(), uuid()));
    std::fs::create_dir_all(dir.join("projects")).unwrap();
    std::fs::write(
        dir.join("welcome.md"),
        "---\ntitle: Welcome\ntags: [mcp, rust]\naliases: [home]\n---\n# Welcome\nStart with the [[Rust Guide]].\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("projects/rust-guide.md"),
        "# Rust Guide\nRust is a fast systems language.\nLinks back to [[Home]].\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("projects/cooking.md"),
        "# Cooking\nBoil pasta. Nothing about compilers here.\n",
    )
    .unwrap();
    dir
}

fn uuid() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
}

struct Client {
    child: std::process::Child,
    reader: BufReader<std::process::ChildStdout>,
}

impl Client {
    fn spawn(vault: &std::path::Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_rustidian"))
            .arg(vault)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn server");
        let stdout = child.stdout.take().unwrap();
        Client {
            child,
            reader: BufReader::new(stdout),
        }
    }

    fn call(&mut self, value: serde_json::Value) -> serde_json::Value {
        let mut stdin = self.child.stdin.as_mut().unwrap();
        writeln!(stdin, "{value}").unwrap();
        stdin.flush().unwrap();
        let mut line = String::new();
        self.reader.read_line(&mut line).unwrap();
        serde_json::from_str(line.trim()).expect("valid response json")
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn init() -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {"protocolVersion": "2024-11-05", "capabilities": {}}
    })
}

fn tool(id: u64, name: &str, args: serde_json::Value) -> serde_json::Value {
    serde_json::json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {"name": name, "arguments": args}})
}

fn text_of(resp: &serde_json::Value) -> String {
    resp["result"]["content"][0]["text"]
        .as_str()
        .expect("text content")
        .to_string()
}

#[test]
fn full_session() {
    let vault = temp_vault();
    let mut c = Client::spawn(&vault);

    let resp = c.call(init());
    assert_eq!(resp["id"], 1);
    assert_eq!(resp["result"]["serverInfo"]["name"], "rustidian");

    let resp = c.call(serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}));
    let names: Vec<&str> = resp["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"search"));
    assert!(names.contains(&"backlinks"));
    assert_eq!(names.len(), 8);

    let resp = c.call(tool(3, "list_notes", serde_json::json!({})));
    let text = text_of(&resp);
    assert!(text.contains("welcome.md — Welcome"));
    assert!(text.contains("[mcp, rust]"));

    let resp = c.call(tool(4, "read_note", serde_json::json!({"path": "welcome.md"})));
    assert!(text_of(&resp).contains("[[Rust Guide]]"));

    let resp = c.call(tool(
        5,
        "search",
        serde_json::json!({"query": "rust systems language"}),
    ));
    let text = text_of(&resp);
    let guide = text.find("projects/rust-guide.md").expect("guide in results");
    let welcome = text.find("welcome.md").expect("welcome in results");
    assert!(
        guide < welcome,
        "rust guide should rank first, got: {text}"
    );

    let resp = c.call(tool(
        6,
        "backlinks",
        serde_json::json!({"path": "welcome.md"}),
    ));
    assert!(text_of(&resp).contains("projects/rust-guide.md"));

    let resp = c.call(tool(7, "get_tags", serde_json::json!({})));
    let text = text_of(&resp);
    assert!(text.contains("mcp (1)"));
    assert!(text.contains("rust (1)"));

    let resp = c.call(tool(
        8,
        "write_note",
        serde_json::json!({"path": "journal/2026.md", "content": "# Journal\nNew note."}),
    ));
    assert!(text_of(&resp).contains("wrote journal/2026.md"));

    let resp = c.call(tool(9, "read_note", serde_json::json!({"path": "journal/2026.md"})));
    assert!(text_of(&resp).contains("New note."));

    let resp = c.call(tool(10, "delete_note", serde_json::json!({"path": "journal/2026.md"})));
    assert!(text_of(&resp).contains("deleted"));

    let resp = c.call(tool(11, "read_note", serde_json::json!({"path": "journal/2026.md"})));
    assert_eq!(resp["result"]["isError"], true);

    let resp = c.call(tool(
        12,
        "read_note",
        serde_json::json!({"path": "../escape.md"}),
    ));
    assert_eq!(resp["result"]["isError"], true);

    let resp = c.call(serde_json::json!({"jsonrpc": "2.0", "id": 13, "method": "no/such"}));
    assert_eq!(resp["error"]["code"], -32601);

    drop(c);
    std::fs::remove_dir_all(&vault).unwrap();
}

#[test]
fn notification_gets_no_response() {
    let vault = temp_vault();
    let mut c = Client::spawn(&vault);
    let _ = c.call(init());
    let mut stdin = c.child.stdin.as_mut().unwrap();
    writeln!(stdin, "{{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}}").unwrap();
    stdin.flush().unwrap();
    // feed another request; the next response we read must be for it
    let resp = c.call(serde_json::json!({"jsonrpc": "2.0", "id": 9, "method": "ping"}));
    assert_eq!(resp["id"], 9);
    drop(c);
    std::fs::remove_dir_all(&vault).unwrap();
}
