# Cditor HTTP Server

HTTP API server that exposes Cditor functionality for integration with editors like Zed.

## Quick Start

```bash
# Start the server
cargo run -p cditor-http-server

# Or with optimized build
cargo run -p cditor-http-server --release
```

The server will start at `http://127.0.0.1:3737`

## API Endpoints

### Health Check
```bash
GET /health
```

Returns server status and available features.

**Example:**
```bash
curl http://127.0.0.1:3737/health
```

### Import Document
```bash
POST /api/import
Content-Type: application/json

{
  "source": "path/to/file.md",
  "source_type": "file"
}
```

Or import from content:
```json
{
  "source": "# Hello\n\nMarkdown content here",
  "source_type": "content"
}
```

**Example:**
```bash
curl -X POST http://127.0.0.1:3737/api/import \
  -H "Content-Type: application/json" \
  -d '{"source": "README.md", "source_type": "file"}'
```

### Export Document
```bash
POST /api/export
Content-Type: application/json

{
  "document_id": "doc_1234567890",
  "format": "markdown"
}
```

**Example:**
```bash
curl -X POST http://127.0.0.1:3737/api/export \
  -H "Content-Type: application/json" \
  -d '{"document_id": "doc_1234567890", "format": "markdown"}'
```

### List Documents
```bash
GET /api/documents
```

**Example:**
```bash
curl http://127.0.0.1:3737/api/documents
```

### Delete Document
```bash
DELETE /api/documents/:id
```

**Example:**
```bash
curl -X DELETE http://127.0.0.1:3737/api/documents/doc_1234567890
```

## Configuration

### Port
The server runs on port 3737 by default. To change it, modify the `addr` variable in `main.rs`.

### Logging
Set the `RUST_LOG` environment variable:

```bash
RUST_LOG=debug cargo run -p cditor-http-server
RUST_LOG=cditor_http_server=trace cargo run -p cditor-http-server
```

## Architecture

The server currently uses in-memory storage for simplicity. For production use, integrate with `cditor-storage` to support:
- SQLite persistence
- PostgreSQL persistence
- Document transactions and history

## Integration with Zed

This server is designed to work with the Zed extension located at `extensions/zed-cditor/`.

1. Start this HTTP server
2. Install the Zed extension in Zed
3. Use slash commands in Zed's Assistant panel:
   - `/cditor-status` - Check server connection
   - `/cditor-import file.md` - Import a file
   - `/cditor-list` - List documents
   - `/cditor-export doc_id` - Export a document

See `doc/guides/ZED_EXTENSION_README.md` for complete integration guide.

## Development

### Hot Reload
For faster development iteration:

```bash
cargo watch -x 'run -p cditor-http-server'
```

### Testing
```bash
# Run tests
cargo test -p cditor-http-server

# Manual testing with curl
./scripts/test_http_server.sh  # (if available)
```

## Future Enhancements

- [ ] Persistent storage integration
- [ ] WebSocket support for real-time updates
- [ ] Authentication and authorization
- [ ] Rate limiting
- [ ] Document versioning API
- [ ] Full-text search endpoint
- [ ] Batch import/export operations
- [ ] Support for more export formats (HTML, PDF, JSON)

## License

GPL-3.0-or-later (same as Cditor project)
