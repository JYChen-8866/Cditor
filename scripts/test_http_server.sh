#!/bin/bash
# Test script for Cditor HTTP Server

set -e

SERVER_URL="http://127.0.0.1:3737"
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo "🧪 Testing Cditor HTTP Server"
echo "Server URL: $SERVER_URL"
echo ""

# Check if server is running
echo -n "1. Testing health endpoint... "
if curl -s -f "$SERVER_URL/health" > /dev/null 2>&1; then
    echo -e "${GREEN}✓ OK${NC}"
    HEALTH_RESPONSE=$(curl -s "$SERVER_URL/health")
    echo "   Response: $HEALTH_RESPONSE"
else
    echo -e "${RED}✗ FAILED${NC}"
    echo -e "${YELLOW}   Make sure the server is running: cargo run -p cditor-http-server${NC}"
    exit 1
fi

echo ""

# Test import with content
echo -n "2. Testing import (content)... "
IMPORT_RESPONSE=$(curl -s -X POST "$SERVER_URL/api/import" \
    -H "Content-Type: application/json" \
    -d '{"source": "# Test Document\n\nThis is a test.\n\n## Features\n\n- Item 1\n- Item 2\n\n```rust\nfn main() {\n    println!(\"Hello\");\n}\n```", "source_type": "content"}')

if echo "$IMPORT_RESPONSE" | grep -q '"success":true'; then
    echo -e "${GREEN}✓ OK${NC}"
    DOC_ID=$(echo "$IMPORT_RESPONSE" | grep -o '"document_id":"[^"]*"' | cut -d'"' -f4)
    echo "   Document ID: $DOC_ID"
    STATS=$(echo "$IMPORT_RESPONSE" | grep -o '"stats":{[^}]*}')
    echo "   Stats: $STATS"
else
    echo -e "${RED}✗ FAILED${NC}"
    echo "   Response: $IMPORT_RESPONSE"
    exit 1
fi

echo ""

# Test list documents
echo -n "3. Testing list documents... "
LIST_RESPONSE=$(curl -s "$SERVER_URL/api/documents")
if echo "$LIST_RESPONSE" | grep -q "$DOC_ID"; then
    echo -e "${GREEN}✓ OK${NC}"
    DOC_COUNT=$(echo "$LIST_RESPONSE" | grep -o '"id"' | wc -l)
    echo "   Total documents: $DOC_COUNT"
else
    echo -e "${RED}✗ FAILED${NC}"
    echo "   Response: $LIST_RESPONSE"
    exit 1
fi

echo ""

# Test export
echo -n "4. Testing export... "
EXPORT_RESPONSE=$(curl -s -X POST "$SERVER_URL/api/export" \
    -H "Content-Type: application/json" \
    -d "{\"document_id\":\"$DOC_ID\",\"format\":\"markdown\"}")

if echo "$EXPORT_RESPONSE" | grep -q '"success":true'; then
    echo -e "${GREEN}✓ OK${NC}"
    CONTENT_LENGTH=$(echo "$EXPORT_RESPONSE" | grep -o '"content":"[^"]*"' | wc -c)
    echo "   Content length: ~$CONTENT_LENGTH chars"
else
    echo -e "${RED}✗ FAILED${NC}"
    echo "   Response: $EXPORT_RESPONSE"
    exit 1
fi

echo ""

# Test delete
echo -n "5. Testing delete... "
DELETE_RESPONSE=$(curl -s -w "%{http_code}" -X DELETE "$SERVER_URL/api/documents/$DOC_ID")
if echo "$DELETE_RESPONSE" | grep -q "204"; then
    echo -e "${GREEN}✓ OK${NC}"
    echo "   Document deleted successfully"
else
    echo -e "${YELLOW}⚠ Warning${NC}"
    echo "   HTTP Status: $DELETE_RESPONSE"
fi

echo ""

# Verify deletion
echo -n "6. Verifying deletion... "
LIST_RESPONSE=$(curl -s "$SERVER_URL/api/documents")
if echo "$LIST_RESPONSE" | grep -q "$DOC_ID"; then
    echo -e "${RED}✗ FAILED${NC}"
    echo "   Document still exists after deletion"
    exit 1
else
    echo -e "${GREEN}✓ OK${NC}"
    echo "   Document successfully removed"
fi

echo ""
echo -e "${GREEN}✅ All tests passed!${NC}"
echo ""
echo "Next steps:"
echo "1. Install the Zed extension: extensions/zed-cditor"
echo "2. Try commands in Zed Assistant:"
echo "   /cditor-status"
echo "   /cditor-import README.md"
echo "   /cditor-list"
