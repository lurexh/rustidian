use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

pub enum Kind {
    Servers,
    OpenCode,
    CodexToml,
}

pub struct Target {
    pub label: &'static str,
    pub path: PathBuf,
    pub kind: Kind,
}

pub fn home() -> PathBuf {
    PathBuf::from(
        std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_default(),
    )
}

pub fn targets(home: &Path) -> Vec<Target> {
    let mut v = Vec::new();
    v.push(Target {
        label: "Claude Code",
        path: home.join(".claude.json"),
        kind: Kind::Servers,
    });
    v.push(Target {
        label: "Cursor",
        path: home.join(".cursor/mcp.json"),
        kind: Kind::Servers,
    });
    let desktop = if cfg!(target_os = "macos") {
        home.join("Library/Application Support/Claude/claude_desktop_config.json")
    } else {
        home.join(".config/Claude/claude_desktop_config.json")
    };
    v.push(Target {
        label: "Claude Desktop",
        path: desktop,
        kind: Kind::Servers,
    });
    if home.join(".config/opencode").is_dir() {
        v.push(Target {
            label: "opencode",
            path: home.join(".config/opencode/opencode.json"),
            kind: Kind::OpenCode,
        });
    }
    if home.join(".codex").is_dir() {
        v.push(Target {
            label: "Codex CLI",
            path: home.join(".codex/config.toml"),
            kind: Kind::CodexToml,
        });
    }
    v
}

fn io_err(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::Other, msg.to_string())
}

fn json_upsert(
    path: &Path,
    top_key: &str,
    name: &str,
    entry: Value,
) -> io::Result<&'static str> {
    let exists = path.exists();
    if !exists {
        let parent = path.parent().ok_or_else(|| io_err("no parent"))?;
        if !parent.is_dir() {
            return Err(io_err("client not installed (config dir missing)"));
        }
    }
    let mut root: Value = if exists {
        let text = fs::read_to_string(path)?;
        if text.trim().is_empty() {
            json!({})
        } else {
            serde_json::from_str(&text).map_err(|e| io_err(&format!("{path:?}: {e}")))?
        }
    } else {
        json!({})
    };
    let map = root
        .as_object_mut()
        .ok_or_else(|| io_err("config root is not an object"))?;
    let section = map
        .entry(top_key.to_string())
        .or_insert_with(|| json!({}));
    let section = section
        .as_object_mut()
        .ok_or_else(|| io_err(format!("{top_key} is not an object").as_str()))?;
    section.insert(name.to_string(), entry);
    let new_text = serde_json::to_string_pretty(&root)? + "\n";
    if exists {
        let old_text = fs::read_to_string(path)?;
        if old_text == new_text {
            return Ok("unchanged");
        }
        let mut bak = path.as_os_str().to_owned();
        bak.push(".bak");
        let _ = fs::copy(path, PathBuf::from(bak));
    }
    fs::write(path, new_text)?;
    if exists {
        Ok("updated")
    } else {
        Ok("created")
    }
}

fn json_remove(path: &Path, top_key: &str, name: &str) -> io::Result<&'static str> {
    if !path.exists() {
        return Err(io_err("config file missing"));
    }
    let text = fs::read_to_string(path)?;
    let mut root: Value =
        serde_json::from_str(&text).map_err(|e| io_err(&format!("{path:?}: {e}")))?;
    let removed = root
        .as_object_mut()
        .and_then(|m| m.get_mut(top_key))
        .and_then(|s| s.as_object_mut())
        .map(|s| s.remove(name).is_some())
        .unwrap_or(false);
    if !removed {
        return Ok("not present");
    }
    let mut bak = path.as_os_str().to_owned();
    bak.push(".bak");
    let _ = fs::copy(path, PathBuf::from(bak));
    fs::write(path, serde_json::to_string_pretty(&root)? + "\n")?;
    Ok("removed")
}

fn toml_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn toml_block(name: &str, command: &str, args: &[String]) -> String {
    let list = args
        .iter()
        .map(|a| format!("\"{}\"", toml_escape(a)))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "[mcp_servers.{name}]\ncommand = \"{}\"\nargs = [{list}]\n",
        toml_escape(command)
    )
}

fn toml_upsert(path: &Path, name: &str, command: &str, args: &[String]) -> io::Result<&'static str> {
    let exists = path.exists();
    if !exists {
        let parent = path.parent().ok_or_else(|| io_err("no parent"))?;
        if !parent.is_dir() {
            return Err(io_err("client not installed (config dir missing)"));
        }
    }
    let block = toml_block(name, command, args);
    let header = format!("[mcp_servers.{name}]");
    let new_text = if exists {
        let old = fs::read_to_string(path)?;
        let mut lines: Vec<&str> = old.lines().collect();
        if let Some(start) = lines.iter().position(|l| l.trim() == header) {
            let mut end = start + 1;
            while end < lines.len() && !lines[end].trim_start().starts_with('[') {
                end += 1;
            }
            lines.splice(start..end, block.lines());
            lines.join("\n") + "\n"
        } else {
            let mut out = old.clone();
            if !out.ends_with('\n') {
                out.push('\n');
            }
            out.push('\n');
            out.push_str(&block);
            out
        }
    } else {
        block
    };
    if exists {
        if fs::read_to_string(path)? == new_text {
            return Ok("unchanged");
        }
        let mut bak = path.as_os_str().to_owned();
        bak.push(".bak");
        let _ = fs::copy(path, PathBuf::from(bak));
    }
    fs::write(path, new_text)?;
    if exists {
        Ok("updated")
    } else {
        Ok("created")
    }
}

fn toml_remove(path: &Path, name: &str) -> io::Result<&'static str> {
    if !path.exists() {
        return Err(io_err("config file missing"));
    }
    let old = fs::read_to_string(path)?;
    let header = format!("[mcp_servers.{name}]");
    let mut lines: Vec<&str> = old.lines().collect();
    let Some(start) = lines.iter().position(|l| l.trim() == header) else {
        return Ok("not present");
    };
    let mut end = start + 1;
    while end < lines.len() && !lines[end].trim_start().starts_with('[') {
        end += 1;
    }
    lines.drain(start..end);
    let mut bak = path.as_os_str().to_owned();
    bak.push(".bak");
    let _ = fs::copy(path, PathBuf::from(bak));
    fs::write(path, lines.join("\n") + "\n")?;
    Ok("removed")
}

fn entry(kind: &Kind, exe: &str, vault: &str) -> Value {
    match kind {
        Kind::Servers => json!({"command": exe, "args": [vault]}),
        Kind::OpenCode => json!({"command": exe, "args": [vault], "enabled": true}),
        Kind::CodexToml => Value::Null,
    }
}

const SERVER_NAME: &str = "rustidian";

pub fn install(vault: &str) -> io::Result<Vec<(String, String)>> {
    let vault = fs::canonicalize(vault)?;
    let vault = vault.to_string_lossy().replace('\\', "/");
    let exe = std::env::current_exe()?
        .to_string_lossy()
        .replace('\\', "/");
    let mut results = Vec::new();
    for t in targets(&home()) {
        let dir_exists = t.path.parent().map_or(false, |p| p.is_dir());
        let outcome = match &t.kind {
            Kind::Servers => {
                if dir_exists || t.path.exists() {
                    json_upsert(&t.path, "mcpServers", SERVER_NAME, entry(&t.kind, &exe, &vault))
                        .map(|s| s.to_string())
                } else {
                    Err(io_err("not installed (config missing)"))
                }
            }
            Kind::OpenCode => {
                if dir_exists || t.path.exists() {
                    json_upsert(&t.path, "mcp", SERVER_NAME, entry(&t.kind, &exe, &vault))
                        .map(|s| s.to_string())
                } else {
                    Err(io_err("not installed (config missing)"))
                }
            }
            Kind::CodexToml => {
                if dir_exists || t.path.exists() {
                    toml_upsert(&t.path, SERVER_NAME, &exe, &[vault.clone()])
                        .map(|s| s.to_string())
                } else {
                    Err(io_err("not installed (config missing)"))
                }
            }
        };
        match outcome {
            Ok(status) => results.push((t.label.to_string(), status)),
            Err(e) => results.push((t.label.to_string(), format!("skipped ({e})"))),
        }
    }
    Ok(results)
}

pub fn uninstall() -> io::Result<Vec<(String, String)>> {
    let mut results = Vec::new();
    for t in targets(&home()) {
        let outcome = match &t.kind {
            Kind::Servers => json_remove(&t.path, "mcpServers", SERVER_NAME),
            Kind::OpenCode => json_remove(&t.path, "mcp", SERVER_NAME),
            Kind::CodexToml => toml_remove(&t.path, SERVER_NAME),
        };
        match outcome {
            Ok(status) => results.push((t.label.to_string(), status.to_string())),
            Err(e) => results.push((t.label.to_string(), format!("skipped ({e})"))),
        }
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_home(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("omr-inst-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn creates_and_updates_mcp_servers_entry() {
        let home = temp_home("srv");
        let cfg = home.join(".cursor/mcp.json");
        fs::create_dir_all(cfg.parent().unwrap()).unwrap();
        fs::write(&cfg, r#"{"other":{"keep":true}}"#).unwrap();

        let entry = json!({"command": "/bin/omc", "args": ["/v"]});
        assert_eq!(json_upsert(&cfg, "mcpServers", "rustidian", entry.clone()).unwrap(), "updated");
        let v: Value = serde_json::from_str(&fs::read_to_string(&cfg).unwrap()).unwrap();
        assert_eq!(v["other"]["keep"], true);
        assert_eq!(v["mcpServers"]["rustidian"], entry);

        let new_entry = json!({"command": "/bin/omc2", "args": ["/v2"]});
        json_upsert(&cfg, "mcpServers", "rustidian", new_entry).unwrap();
        let v: Value = serde_json::from_str(&fs::read_to_string(&cfg).unwrap()).unwrap();
        assert_eq!(v["mcpServers"]["rustidian"]["command"], "/bin/omc2");
        assert!(home.join(".cursor/mcp.json.bak").exists());
        fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn refuses_to_create_when_client_missing() {
        let home = temp_home("miss");
        let cfg = home.join(".cursor/mcp.json");
        let r = json_upsert(&cfg, "mcpServers", "rustidian", json!({}));
        assert!(r.is_err());
        fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn toml_upsert_appends_and_replaces() {
        let home = temp_home("toml");
        let cfg = home.join(".codex/config.toml");
        fs::create_dir_all(cfg.parent().unwrap()).unwrap();
        fs::write(&cfg, "[other]\nkey = 1\n").unwrap();

        toml_upsert(&cfg, "rustidian", "/bin/omc", &["/v".into()]).unwrap();
        let text = fs::read_to_string(&cfg).unwrap();
        assert!(text.contains("[other]"));
        assert!(text.contains("[mcp_servers.rustidian]"));
        assert!(text.contains("command = \"/bin/omc\""));
        assert!(text.contains("args = [\"/v\"]"));

        toml_upsert(&cfg, "rustidian", "/bin/omc2", &["/v2".into()]).unwrap();
        let text = fs::read_to_string(&cfg).unwrap();
        assert!(text.contains("command = \"/bin/omc2\""));
        assert!(!text.contains("/bin/omc\""));
        assert!(text.contains("[other]"));
        fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn uninstall_removes_entries() {
        let home = temp_home("rm");
        let cfg = home.join(".cursor/mcp.json");
        fs::create_dir_all(cfg.parent().unwrap()).unwrap();
        fs::write(&cfg, r#"{"mcpServers":{"rustidian":{"command":"x","args":[]},"keep":{"command":"y"}}}"#).unwrap();
        assert_eq!(json_remove(&cfg, "mcpServers", "rustidian").unwrap(), "removed");
        let v: Value = serde_json::from_str(&fs::read_to_string(&cfg).unwrap()).unwrap();
        assert!(v["mcpServers"].get("rustidian").is_none());
        assert_eq!(v["mcpServers"]["keep"]["command"], "y");
        fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn toml_escape_works() {
        assert_eq!(toml_escape("a\"b\\c"), "a\\\"b\\\\c");
    }
}
