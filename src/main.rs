mod frontmatter;
mod install;
mod search;
mod vault;

use std::io::{self, BufRead, Write};

use serde_json::{json, Value};

use vault::Vault;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn tool_list() -> Value {
    json!({
        "tools": [
            {"name": "list_notes", "description": "List all markdown notes in the vault with title, tags and aliases. Optional `filter` matches paths.", "inputSchema": {"type": "object", "properties": {"filter": {"type": "string"}}}},
            {"name": "read_note", "description": "Read the raw markdown content of a note by path relative to the vault root.", "inputSchema": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}},
            {"name": "write_note", "description": "Create or overwrite a note. Parent folders are created automatically.", "inputSchema": {"type": "object", "properties": {"path": {"type": "string"}, "content": {"type": "string"}}, "required": ["path", "content"]}},
            {"name": "delete_note", "description": "Delete a note by path relative to the vault root.", "inputSchema": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}},
            {"name": "search", "description": "Ranked full-text search across all notes (TF-IDF + substring + filename boosts). Returns path, score and a matching snippet.", "inputSchema": {"type": "object", "properties": {"query": {"type": "string"}, "limit": {"type": "integer", "minimum": 1, "maximum": 100}}, "required": ["query"]}},
            {"name": "backlinks", "description": "List notes whose [[wikilinks]] resolve to the given note.", "inputSchema": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}},
            {"name": "get_tags", "description": "All frontmatter tags in the vault with usage counts, sorted by frequency.", "inputSchema": {"type": "object", "properties": {}}},
            {"name": "recent_notes", "description": "Notes sorted by last modification time, newest first.", "inputSchema": {"type": "object", "properties": {"limit": {"type": "integer", "minimum": 1, "maximum": 100}}}}
        ]
    })
}

fn fmt_summary(s: &vault::NoteSummary) -> String {
    let mut line = s.path.clone();
    if let Some(t) = &s.title {
        line.push_str(&format!(" — {t}"));
    }
    if !s.tags.is_empty() {
        line.push_str("  [");
        line.push_str(&s.tags.join(", "));
        line.push(']');
    }
    if !s.aliases.is_empty() {
        line.push_str("  ~");
        line.push_str(&s.aliases.join(", ~"));
    }
    line
}

fn tool_text(text: String) -> Value {
    json!({"content": [{"type": "text", "text": text}], "isError": false})
}

fn tool_error(msg: String) -> Value {
    json!({"content": [{"type": "text", "text": msg}], "isError": true})
}

fn call_tool(vault: &Vault, params: &Value) -> Value {
    let name = params["name"].as_str().unwrap_or_default().to_string();
    let args = params["arguments"].as_object().cloned().unwrap_or_default();
    let arg = |k: &str| args.get(k).and_then(Value::as_str).unwrap_or("");
    let num = |k: &str| args.get(k).and_then(Value::as_u64).unwrap_or(0) as usize;

    match name.as_str() {
        "list_notes" => match vault.list(args.get("filter").and_then(Value::as_str)) {
            Ok(notes) => tool_text(if notes.is_empty() {
                "(no notes)".into()
            } else {
                notes.iter().map(fmt_summary).collect::<Vec<_>>().join("\n")
            }),
            Err(e) => tool_error(e.to_string()),
        },
        "read_note" => {
            let path = arg("path");
            if path.is_empty() {
                tool_error("missing `path`".into())
            } else {
                match vault.read(path) {
                    Ok(content) => tool_text(content),
                    Err(e) => tool_error(format!("{path}: {e}")),
                }
            }
        }
        "write_note" => {
            let (path, content) = (arg("path"), arg("content"));
            if path.is_empty() {
                tool_error("missing `path`".into())
            } else {
                match vault.write(path, content) {
                    Ok(()) => tool_text(format!("wrote {path} ({} bytes)", content.len())),
                    Err(e) => tool_error(format!("{path}: {e}")),
                }
            }
        }
        "delete_note" => {
            let path = arg("path");
            if path.is_empty() {
                tool_error("missing `path`".into())
            } else {
                match vault.delete(path) {
                    Ok(()) => tool_text(format!("deleted {path}")),
                    Err(e) => tool_error(format!("{path}: {e}")),
                }
            }
        }
        "search" => {
            let query = arg("query");
            if query.is_empty() {
                tool_error("missing `query`".into())
            } else {
                let limit = num("limit");
                match vault.search(query, if limit == 0 { 20 } else { limit }) {
                    Ok(hits) if hits.is_empty() => tool_text("(no matches)".into()),
                    Ok(hits) => tool_text(
                        hits.iter()
                            .map(|h| {
                                let mut line = format!("{:.3}  {}", h.score, h.path);
                                if !h.snippet.is_empty() {
                                    line.push_str("\n    ");
                                    line.push_str(&h.snippet);
                                }
                                line
                            })
                            .collect::<Vec<_>>()
                            .join("\n"),
                    ),
                    Err(e) => tool_error(e.to_string()),
                }
            }
        }
        "backlinks" => {
            let path = arg("path");
            if path.is_empty() {
                tool_error("missing `path`".into())
            } else {
                match vault.backlinks(path) {
                    Ok(links) if links.is_empty() => tool_text(format!("(no links to {path})")),
                    Ok(links) => tool_text(links.join("\n")),
                    Err(e) => tool_error(e.to_string()),
                }
            }
        }
        "get_tags" => match vault.tags() {
            Ok(tags) if tags.is_empty() => tool_text("(no tags)".into()),
            Ok(tags) => tool_text(
                tags.iter()
                    .map(|(t, n)| format!("{t} ({n})"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
            Err(e) => tool_error(e.to_string()),
        },
        "recent_notes" => match vault.recent(if num("limit") == 0 { 10 } else { num("limit") }) {
            Ok(notes) if notes.is_empty() => tool_text("(no notes)".into()),
            Ok(notes) => tool_text(
                notes
                    .iter()
                    .map(|s| format!("{} {}", s.mtime, fmt_summary(s)))
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
            Err(e) => tool_error(e.to_string()),
        },
        _ => tool_error(format!("unknown tool: {name}")),
    }
}

fn dispatch(vault: &Vault, method: &str, params: &Value) -> Result<Option<Value>, Value> {
    match method {
        "initialize" => Ok(Some(json!({
            "protocolVersion": params["protocolVersion"].as_str().unwrap_or("2024-11-05"),
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "rustidian", "version": VERSION}
        }))),
        "ping" => Ok(Some(json!({}))),
        "tools/list" => Ok(Some(tool_list())),
        "tools/call" => Ok(Some(call_tool(vault, params))),
        "resources/list" => Ok(Some(json!({"resources": []}))),
        "prompts/list" => Ok(Some(json!({"prompts": []}))),
        m if m.starts_with("notifications/") => Ok(None),
        _ => Err(json!({"code": -32601, "message": format!("method not found: {method}")})),
    }
}

fn run() -> i32 {
    let mut args = std::env::args().skip(1);
    let Some(first) = args.next() else {
        print_usage();
        return 2;
    };
    match first.as_str() {
        "install" => {
            let Some(vault) = args.next() else {
                eprintln!("usage: rustidian install <vault-path>");
                return 2;
            };
            match install::install(&vault) {
                Ok(results) => {
                    println!("wired rustidian MCP server into:");
                    for (label, status) in &results {
                        println!("  {label}: {status}");
                    }
                    println!("vault: {}", std::fs::canonicalize(&vault).map_or_else(|_| vault.clone(), |p| p.display().to_string()));
                    0
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    1
                }
            }
        }
        "uninstall" => match install::uninstall() {
            Ok(results) => {
                println!("removed rustidian MCP server from:");
                for (label, status) in &results {
                    println!("  {label}: {status}");
                }
                0
            }
            Err(e) => {
                eprintln!("error: {e}");
                1
            }
        },
        "-h" | "--help" | "help" => {
            print_usage();
            0
        }
        "-V" | "--version" => {
            println!("rustidian {VERSION}");
            0
        }
        _ => {
            let vault = first;
            let vault_server = match Vault::open(&vault) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("error: cannot open vault {vault:?}: {e}");
                    return 1;
                }
            };
            serve(vault_server)
        }
    }
}

fn print_usage() {
    eprintln!(
        "usage:\n  rustidian install <vault-path>   wire into detected MCP clients\n  rustidian uninstall              remove from MCP clients\n  rustidian <vault-path>           run the MCP server on a vault (stdio)"
    );
}

fn serve(vault: Vault) -> i32 {
    let stdin = io::stdin();
    let mut out = io::stdout().lock();
    let mut line = String::new();

    loop {
        line.clear();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => return 0,
            Ok(_) => {}
            Err(_) => return 1,
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(req) = serde_json::from_str::<Value>(trimmed) else {
            let _ = writeln!(
                out,
                "{}",
                json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": "parse error"}})
            );
            let _ = out.flush();
            continue;
        };

        let id = req["id"].clone();
        let method = req["method"].as_str().unwrap_or_default().to_string();
        let params = req["params"].as_object().cloned().unwrap_or_default();

        let is_notification = id.is_null();
        match dispatch(&vault, &method, &Value::Object(params)) {
            Ok(None) => {}
            Ok(Some(result)) => {
                if !is_notification {
                    let _ = writeln!(
                        out,
                        "{}",
                        json!({"jsonrpc": "2.0", "id": id, "result": result})
                    );
                    let _ = out.flush();
                }
            }
            Err(err) => {
                if !is_notification {
                    let _ = writeln!(
                        out,
                        "{}",
                        json!({"jsonrpc": "2.0", "id": id, "error": err})
                    );
                    let _ = out.flush();
                }
            }
        }
    }
}

fn main() {
    std::process::exit(run());
}
