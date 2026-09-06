# Cditor Zed Extension

Integrate Cditor's rich-text document capabilities into Zed editor through slash commands.

## Features

- 📥 Import Markdown files to Cditor format
- 📤 Export Cditor documents as Markdown
- 📋 List all stored documents
- 🗑️ Delete documents
- ✅ Server health check

## Installation

### Step 1: Start the HTTP Server

The extension requires a local HTTP server to be running:

```bash
# From the CDitor root directory
cargo run -p cditor-http-server

# Or use the convenience script
./scripts/start_zed_server.sh
```

The server will start at `http://127.0.0.1:3737`

### Step 2: Install Extension in Zed

1. Open Zed editor
2. Press `Cmd+Shift+P` (macOS) or `Ctrl+Shift+P` (Windows/Linux)
3. Type and select: `zed: install dev extension`
4. Navigate to and select: `/path/to/CDitor/extensions/zed-cditor`
5. The extension will be installed in development mode

### Step 3: Verify Installation

Open Zed's Assistant panel and try:

```
/cditor-status
```

You should see:
```
✅ Cditor server is running

Version: 0.2.6
Status: ok
Features: import, export, markdown, tables

Server URL: http://127.0.0.1:3737
```

## Available Commands

### `/cditor-status`
Check if the Cditor HTTP server is running.

**Example:**
```
/cditor-status
```

### `/cditor-import <file_path>`
Import a Markdown file into Cditor format.

**Example:**
```
/cditor-import README.md
/cditor-import ~/Documents/notes.md
```

**Output:**
```
✅ Successfully imported file: README.md

Document ID: doc_1725283920
Blocks: 45
Has tables: yes
Has code blocks: yes
Has images: no

Use /cditor-export doc_1725283920 to export this document
Use /cditor-list to see all documents
```

### `/cditor-list`
List all documents stored in Cditor.

**Example:**
```
/cditor-list
```

**Output:**
```
📚 Cditor Documents (2 total)

📄 Cditor
• ID: doc_1725283920
• Blocks: 45
• Created: 2024-09-02T10:25:20Z
• Modified: 2024-09-02T10:25:20Z

📄 Project Notes
• ID: doc_1725284100
• Blocks: 23
• Created: 2024-09-02T10:28:20Z
• Modified: 2024-09-02T10:28:20Z

Use /cditor-export <document_id> to export a document
```

### `/cditor-export <document_id>`
Export a Cditor document as Markdown.

**Example:**
```
/cditor-export doc_1725283920
```

**Output:**
```
✅ Document exported successfully

```markdown
# Cditor

English | [简体中文](README.zh-CN.md)

Cditor is an open-source, block-based rich-text editor...
```
```

### `/cditor-delete <document_id>`
Delete a document from Cditor storage.

**Example:**
```
/cditor-delete doc_1725283920
```

**Output:**
```
✅ Document deleted successfully: doc_1725283920

Use /cditor-list to see remaining documents
```

## Typical Workflow

1. **Check server status:**
   ```
   /cditor-status
   ```

2. **Import a Markdown file:**
   ```
   /cditor-import my-notes.md
   ```

3. **View all documents:**
   ```
   /cditor-list
   ```

4. **Export a specific document:**
   ```
   /cditor-export doc_1725283920
   ```

5. **Copy the exported Markdown to use in Zed**

## Troubleshooting

### "Cannot connect to Cditor server"

**Problem:** The HTTP server is not running.

**Solution:**
```bash
# Start the server in a separate terminal
cargo run -p cditor-http-server

# Or use the script
./scripts/start_zed_server.sh
```

### Extension not showing in Zed

**Problem:** The extension wasn't installed correctly.

**Solution:**
1. Make sure you selected the correct directory: `extensions/zed-cditor`
2. Check Zed's Extensions panel (`Cmd+Shift+X`) for any error messages
3. Try reinstalling: remove the dev extension and install again

### Commands not autocompleting

**Problem:** Slash commands don't show up in suggestions.

**Solution:**
- Make sure you're in the Assistant panel (not the editor)
- Type `/cditor-` and wait for suggestions
- Restart Zed if commands still don't appear

### "Failed to parse JSON"

**Problem:** Server returned an unexpected response.

**Solution:**
1. Check server logs for errors
2. Verify the server is running the correct version
3. Restart the server: `Ctrl+C` and run again

## Architecture

```
┌─────────────────┐         ┌──────────────────┐         ┌─────────────────┐
│  Zed Editor     │         │  HTTP Server     │         │  Cditor Core    │
│  (WASM Ext)     │ ──HTTP──▶ (Rust/Axum)     │ ────────▶ (cditor-core)   │
│  /cditor-*      │ ◀────── │  REST API        │ ◀────── │  Doc Processing │
└─────────────────┘         └──────────────────┘         └─────────────────┘
```

The extension communicates with a local HTTP server that handles:
- Document import/export
- Format conversion (Markdown ↔ Cditor)
- Storage management

## Development

### Building the Extension

```bash
cd extensions/zed-cditor
cargo build --target wasm32-wasip2
```

### Testing

1. Make changes to `src/lib.rs`
2. Reload the dev extension in Zed
3. Test with slash commands

### Server Development

Edit `crates/cditor-http-server/src/main.rs` and restart the server:

```bash
cargo run -p cditor-http-server
```

For hot reload:
```bash
cargo watch -x 'run -p cditor-http-server'
```

## Configuration

### Changing Server Port

Edit `extensions/zed-cditor/src/lib.rs`:

```rust
Self {
    server_url: "http://127.0.0.1:YOUR_PORT".to_string(),
}
```

Also update the server in `crates/cditor-http-server/src/main.rs`:

```rust
let addr = "127.0.0.1:YOUR_PORT";
```

## Limitations

- **No UI components:** The extension can only return text to Zed's Assistant panel. Full rich-text editing requires using the Cditor desktop app.
- **Local server required:** The HTTP server must be running for the extension to work.
- **In-memory storage:** Currently documents are stored in memory. Restart the server to clear all documents.

## Future Enhancements

- [ ] Persistent storage (SQLite/PostgreSQL)
- [ ] Auto-start server when Zed launches
- [ ] Document search command
- [ ] Table editing commands
- [ ] AI integration commands
- [ ] Export to more formats (HTML, PDF)
- [ ] Batch operations
- [ ] Document versioning

## Related Documentation

- [Zed Extension Integration Plan](../../doc/guides/zed-extension-integration-plan.md)
- [Zed Extension POC](../../doc/guides/zed-extension-poc.md)
- [HTTP Server README](../../crates/cditor-http-server/README.md)
- [Zed Extension Development Guide](https://zed.dev/docs/extensions/developing-extensions)

## Support

For issues or questions:
- Check the [main Cditor README](../../README.md)
- Review the [integration guide](../../doc/guides/ZED_EXTENSION_README.md)
- Open an issue on GitHub

## License

GPL-3.0-or-later (same as Cditor project)
