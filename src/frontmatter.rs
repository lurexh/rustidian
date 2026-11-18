use std::collections::HashSet;

#[derive(Debug, Default, Clone)]
pub struct Meta {
    pub title: Option<String>,
    pub tags: Vec<String>,
    pub aliases: Vec<String>,
}

pub fn split(content: &str) -> (&str, &str) {
    let body = content.strip_prefix('\u{feff}').unwrap_or(content);
    let mut lines = body.lines();
    match lines.next() {
        Some(line) if line.trim() == "---" => {
            let rest = &body[line.len() + 1..];
            match rest.find("\n---") {
                Some(end) => {
                    let meta = &rest[..end];
                    let after = &rest[end + 4..];
                    let after = after.strip_prefix('\n').unwrap_or(after);
                    (meta, after)
                }
                None => ("", content),
            }
        }
        _ => ("", content),
    }
}

fn scalar(value: &str) -> String {
    let value = value.trim();
    let value = value.strip_prefix('#').unwrap_or(value);
    value.trim_matches(|c| c == '"' || c == '\'').to_string()
}

fn inline_list(value: &str) -> Vec<String> {
    let inner = value.trim().trim_start_matches('[').trim_end_matches(']');
    inner
        .split(',')
        .filter_map(|item| {
            let item = scalar(item);
            if item.is_empty() {
                None
            } else {
                Some(item)
            }
        })
        .collect()
}

pub fn parse(meta: &str) -> Meta {
    let mut out = Meta::default();
    let mut section: Option<String> = None;
    for line in meta.lines() {
        if line.starts_with(' ') || line.starts_with('\t') || line.starts_with("- ") {
            let item = line.trim().trim_start_matches('-').trim();
            let item = scalar(item);
            if item.is_empty() {
                continue;
            }
            match section.as_deref() {
                Some("tags") => out.tags.push(item),
                Some("aliases") => out.aliases.push(item),
                _ => {}
            }
            continue;
        }
        section = None;
        let Some((key, value)) = line.split_once(':') else { continue };
        let key = key.trim().to_lowercase();
        let value = value.trim();
        if value.is_empty() {
            section = Some(key);
            continue;
        }
        match key.as_str() {
            "title" => out.title = Some(scalar(value)),
            "tags" | "tag" => out.tags.extend(inline_list(value)),
            "aliases" | "alias" | "alias_names" => out.aliases.extend(inline_list(value)),
            _ => {}
        }
    }
    out.tags = dedup(out.tags);
    out.aliases = dedup(out.aliases);
    out
}

fn dedup(items: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    items
        .into_iter()
        .filter(|item| !item.is_empty() && seen.insert(item.to_lowercase()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_frontmatter() {
        let (meta, body) = split("---\ntitle: Hi\n---\n# Hi\nbody");
        assert_eq!(meta, "title: Hi");
        assert_eq!(body, "# Hi\nbody");
    }

    #[test]
    fn no_frontmatter() {
        let (meta, body) = split("just a note");
        assert_eq!(meta, "");
        assert_eq!(body, "just a note");
    }

    #[test]
    fn parses_inline_tags() {
        let m = parse("tags: [rust, mcp, #cli]\naliases: [\"Obsidian MCP\", 'omr']");
        assert_eq!(m.tags, vec!["rust", "mcp", "cli"]);
        assert_eq!(m.aliases, vec!["Obsidian MCP", "omr"]);
    }

    #[test]
    fn parses_list_tags_and_title() {
        let m = parse("title: My Note\ntags:\n- a\n- b\naliases:\n- foo");
        assert_eq!(m.title.as_deref(), Some("My Note"));
        assert_eq!(m.tags, vec!["a", "b"]);
        assert_eq!(m.aliases, vec!["foo"]);
    }

    #[test]
    fn dedupes_case_insensitive() {
        let m = parse("tags: [Rust, rust, RUST]");
        assert_eq!(m.tags, vec!["Rust"]);
    }
}
