#!/bin/bash
# Start the Cditor HTTP server for Zed integration

set -e

echo "🚀 Starting Cditor HTTP Server..."
echo ""

# Check if cargo is available
if ! command -v cargo &> /dev/null; then
    echo "❌ Error: cargo not found. Please install Rust."
    exit 1
fi

# Navigate to project root
cd "$(dirname "$0")/../.."

# Start the server
echo "📡 Server will be available at http://127.0.0.1:3737"
echo "📝 Use Ctrl+C to stop the server"
echo ""

cargo run -p cditor-http-server
