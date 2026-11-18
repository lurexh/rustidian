# rustidian

Obsidian vault as an MCP server — a single Rust binary that reads your vault **directly from disk**. No Obsidian running, no plugins, no Python, no Node.

## Install

**One-liner** (downloads latest release binary):

```sh
curl -fsSL https://raw.githubusercontent.com/lurexh/rustidian/main/install.sh | sh
```

**Cargo** (crate name: `rustidian`):

```sh
cargo install rustidian
```

**Homebrew** (after first release):

```sh
brew install lurexh/tap/rustidian
```

**Windows**: grab `rustidian-x86_64-pc-windows-msvc.tar.gz` from [Releases](https://github.com/lurexh/rustidian/releases).

## Wire up your AI clients — one command

```sh
rustidian install /path/to/your/vault
```

That's it. The installer detects which MCP clients you have and registers itself in each one:

| Client | Config touched |
|---|---|
| Claude Code | `~/.claude.json` |
| Claude Desktop | `claude_desktop_config.json` |
| Cursor | `~/.cursor/mcp.json` |
| opencode | `~/.config/opencode/opencode.json` |
| Codex CLI | `~/.codex/config.toml` |

- Existing clients you don't have are skipped
- Configs are backed up to `<file>.bak` before touching
- Remove from everything: `rustidian uninstall`

## Why

Existing Obsidian MCP servers (4.5k★ Python, 3.2k★ TS, ...) require the Obsidian app to be open with a REST plugin installed. `rustidian` just needs the folder:

- Single binary (~450KB stripped)
- One dependency (`serde_json`) — no runtime deps
- Direct file access — works headless, on servers, in CI
- Path-jailed: cannot read or write anything outside the vault
- Built-in incremental index for fast, ranked search

## Manual setup (if you prefer)

```sh
rustidian /path/to/your/vault
```

Speaks MCP over stdio (newline-delimited JSON-RPC). Point any MCP client at it:

```json
{
  "mcpServers": {
    "rustidian": {
      "command": "rustidian",
      "args": ["/path/to/your/vault"]
    }
  }
}
```

## Tools

| Tool | Description |
|---|---|
| `list_notes` | All markdown notes with title, tags, aliases; optional path filter |
| `read_note` | Raw markdown of a note |
| `write_note` | Create or overwrite a note (parent folders auto-created) |
| `delete_note` | Delete a note |
| `search` | Ranked full-text search — TF-IDF cosine + substring + filename boosts, with snippets |
| `backlinks` | Notes whose `[[wikilinks]]` resolve to a given note (matches stems, paths and aliases) |
| `get_tags` | All frontmatter tags with usage counts |
| `recent_notes` | Notes by last modification, newest first |

## Features

- **Frontmatter**: parses `title`, `tags` and `aliases` (inline `[a, b]` and list `- item` styles) — no YAML dependency
- **Wikilinks**: `[[target]]`, `[[target|alias]]`, `[[target#heading]]`; links resolve case-insensitively by full path, note stem, or alias
- **Search ranking**: TF-IDF cosine similarity, boosted by exact substring hits and filename token matches
- **Safety**: relative paths only, `..` and absolute paths rejected, symlink-proof canonical root

## Building from source

```sh
cargo build --release
# binary at target/release/rustidian
```

## Tests

```sh
cargo test
```

17 tests: frontmatter parsing, tokenization, TF-IDF ranking, wikilink extraction, path-jail, installer config editing (JSON + TOML), plus end-to-end integration tests that drive the actual binary over stdio with a synthetic vault.

## Releasing (maintainer)

```sh
git tag v0.1.0 && git push origin v0.1.0
```

The release workflow builds static binaries for Linux (x86_64 + aarch64 musl), macOS (both arches) and Windows, attaches them to a GitHub Release, and optionally:

- publishes to crates.io — set a `CRATES_TOKEN` repo secret
- updates [`lurexh/homebrew-tap`](https://github.com/lurexh/homebrew-tap) — set a `TAP_TOKEN` repo secret

## Roadmap

- [ ] Local semantic search (embeddings, still single-binary)
- [ ] `note://` resources for MCP resource reads
- [ ] Daily-notes helpers
- [ ] Watch mode (inotify/FSEvents)

## License

MIT
