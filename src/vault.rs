use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::frontmatter::{self as front, Meta};
use crate::search::{self, Hit};

pub struct NoteSummary {
    pub path: String,
    pub title: Option<String>,
    pub tags: Vec<String>,
    pub aliases: Vec<String>,
    pub mtime: u64,
}

struct IndexedNote {
    path: String,
    mtime: u64,
    content: String,
    meta: Meta,
    tokens: Vec<String>,
    links: Vec<String>,
}

pub struct Vault {
    root: PathBuf,
    index: RefCell<Vec<IndexedNote>>,
    fresh: Cell<bool>,
}

impl Vault {
    pub fn open(path: &str) -> io::Result<Self> {
        let root = fs::canonicalize(path)?;
        Ok(Self {
            root,
            index: RefCell::new(Vec::new()),
            fresh: Cell::new(false),
        })
    }

    fn rel_path(&self, abs: &Path) -> Option<String> {
        abs.strip_prefix(&self.root)
            .ok()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
    }

    fn check_rel(rel: &str) -> io::Result<()> {
        let p = Path::new(rel);
        if p.is_absolute() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "absolute paths not allowed"));
        }
        for comp in p.components() {
            match comp {
                Component::Normal(_) => {}
                other => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("illegal path component: {other:?}"),
                    ))
                }
            }
        }
        Ok(())
    }

    pub fn refresh(&self) -> io::Result<()> {
        let mut notes = Vec::new();
        let mut stack = vec![self.root.clone()];
        while let Some(dir) = stack.pop() {
            for entry in fs::read_dir(&dir)? {
                let entry = entry?;
                let path = entry.path();
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.starts_with('.') {
                    continue;
                }
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().map_or(false, |e| e == "md") {
                    let meta = entry.metadata()?;
                    let mtime = meta
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    let Ok(content) = fs::read_to_string(&path) else { continue };
                    let Some(rel) = self.rel_path(&path) else { continue };
                    let (meta_raw, _) = front::split(&content);
                    let meta = front::parse(meta_raw);
                    let tokens = search::tokenize(&content);
                    let links = extract_links(&content);
                    notes.push(IndexedNote {
                        path: rel,
                        mtime,
                        content,
                        meta,
                        tokens,
                        links,
                    });
                }
            }
        }
        notes.sort_by(|a, b| a.path.cmp(&b.path));
        *self.index.borrow_mut() = notes;
        self.fresh.set(true);
        Ok(())
    }

    fn refresh_if_needed(&self) -> io::Result<()> {
        if !self.fresh.get() {
            self.refresh()?;
        }
        Ok(())
    }

    pub fn list(&self, filter: Option<&str>) -> io::Result<Vec<NoteSummary>> {
        self.refresh_if_needed()?;
        let index = self.index.borrow();
        let filter = filter.map(|f| f.to_lowercase());
        Ok(index
            .iter()
            .filter(|n| {
                filter
                    .as_ref()
                    .map_or(true, |f| n.path.to_lowercase().contains(f))
            })
            .map(NoteSummary::from)
            .collect())
    }

    pub fn read(&self, rel: &str) -> io::Result<String> {
        Self::check_rel(rel)?;
        fs::read_to_string(self.root.join(rel))
    }

    pub fn write(&self, rel: &str, content: &str) -> io::Result<()> {
        Self::check_rel(rel)?;
        let target = self.root.join(rel);
        if let Some(dir) = target.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(&target, content)?;
        self.fresh.set(false);
        Ok(())
    }

    pub fn delete(&self, rel: &str) -> io::Result<()> {
        Self::check_rel(rel)?;
        let target = self.root.join(rel);
        if !target.exists() {
            return Err(io::Error::new(io::ErrorKind::NotFound, "note not found"));
        }
        fs::remove_file(&target)?;
        self.fresh.set(false);
        Ok(())
    }

    pub fn search(&self, query: &str, limit: usize) -> io::Result<Vec<Hit>> {
        self.refresh_if_needed()?;
        let index = self.index.borrow();
        let query = query.trim();
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let q_tokens = search::tokenize(query);
        let q_lower = query.to_lowercase();
        let docs: Vec<(String, Vec<String>)> = index
            .iter()
            .map(|n| (n.path.clone(), n.tokens.clone()))
            .collect();

        let mut hits: Vec<Hit> = search::tfidf(&docs, &q_tokens);
        let mut in_hits: HashSet<String> = hits.iter().map(|h| h.path.clone()).collect();
        for n in index.iter() {
            let lower = n.content.to_lowercase();
            let substr = lower.contains(&q_lower);
            let name_match = q_tokens
                .iter()
                .all(|t| n.path.to_lowercase().contains(t.as_str()));
            if in_hits.contains(&n.path) {
                if let Some(h) = hits.iter_mut().find(|h| h.path == n.path) {
                    if substr {
                        h.score += 0.5;
                    }
                    if name_match {
                        h.score += 0.3;
                    }
                }
            } else if substr || name_match {
                hits.push(Hit {
                    path: n.path.clone(),
                    score: 0.4 + if name_match { 0.3 } else { 0.0 },
                    snippet: String::new(),
                });
                in_hits.insert(n.path.clone());
            }
        }

        hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        hits.truncate(limit.max(1));
        for h in &mut hits {
            if let Some(n) = index.iter().find(|n| n.path == h.path) {
                h.snippet = search::snippet(&n.content, &q_tokens, 160);
            }
        }
        Ok(hits)
    }

    pub fn resolve_link(&self, target: &str) -> Option<String> {
        let clean = target
            .split('|').next()?
            .split('#').next()?
            .trim()
            .trim_end_matches(".md")
            .to_lowercase();
        if clean.is_empty() {
            return None;
        }
        let index = self.index.borrow();
        for n in index.iter() {
            if n.path.to_lowercase() == clean {
                return Some(n.path.clone());
            }
        }
        for n in index.iter() {
            let stem = n.path.rsplit('/').next().unwrap_or(&n.path);
            if stem.trim_end_matches(".md").to_lowercase() == clean {
                return Some(n.path.clone());
            }
        }
        for n in index.iter() {
            if n.meta.aliases.iter().any(|a| a.to_lowercase() == clean) {
                return Some(n.path.clone());
            }
        }
        None
    }

    pub fn backlinks(&self, rel: &str) -> io::Result<Vec<String>> {
        self.refresh_if_needed()?;
        let mut out = Vec::new();
        let index = self.index.borrow();
        for n in index.iter() {
            if n.path == rel {
                continue;
            }
            if n.links.iter().any(|l| self.resolve_link(l).as_deref() == Some(rel)) {
                out.push(n.path.clone());
            }
        }
        Ok(out)
    }

    pub fn tags(&self) -> io::Result<Vec<(String, u32)>> {
        self.refresh_if_needed()?;
        let index = self.index.borrow();
        let mut counts: HashMap<String, u32> = HashMap::new();
        for n in index.iter() {
            for tag in &n.meta.tags {
                *counts.entry(tag.to_lowercase()).or_default() += 1;
            }
        }
        let mut out: Vec<(String, u32)> = counts.into_iter().collect();
        out.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        Ok(out)
    }

    pub fn recent(&self, limit: usize) -> io::Result<Vec<NoteSummary>> {
        self.refresh_if_needed()?;
        let index = self.index.borrow();
        let mut notes: Vec<&IndexedNote> = index.iter().collect();
        notes.sort_by(|a, b| b.mtime.cmp(&a.mtime));
        Ok(notes
            .into_iter()
            .take(limit.max(1))
            .map(NoteSummary::from)
            .collect())
    }
}

impl From<&IndexedNote> for NoteSummary {
    fn from(n: &IndexedNote) -> Self {
        Self {
            path: n.path.clone(),
            title: n.meta.title.clone(),
            tags: n.meta.tags.clone(),
            aliases: n.meta.aliases.clone(),
            mtime: n.mtime,
        }
    }
}

fn extract_links(content: &str) -> Vec<String> {
    let mut links = Vec::new();
    let bytes: Vec<char> = content.chars().collect();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == '[' && bytes[i + 1] == '[' {
            let mut j = i + 2;
            let mut target = String::new();
            while j < bytes.len() && !(bytes[j] == ']' && j + 1 < bytes.len() && bytes[j + 1] == ']')
            {
                target.push(bytes[j]);
                j += 1;
            }
            if j + 1 < bytes.len() {
                let target = target
                    .split('|')
                    .next()
                    .unwrap_or_default()
                    .split('#')
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                if !target.is_empty() {
                    links.push(target);
                }
                i = j + 2;
            } else {
                i = j;
            }
        } else {
            i += 1;
        }
    }
    links
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_wikilinks() {
        let c = "see [[Rust Book]] and [[Note|alias]], skip [[#section]]";
        assert_eq!(extract_links(c), vec!["Rust Book", "Note"]);
    }

    #[test]
    fn rejects_path_escape() {
        assert!(Vault::check_rel("../evil.md").is_err());
        assert!(Vault::check_rel("/abs.md").is_err());
        assert!(Vault::check_rel("notes/sub/ok.md").is_ok());
    }
}
