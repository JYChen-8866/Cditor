#!/bin/bash
# Build script for DrafftInk

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

print_status() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Parse arguments
BUILD_TYPE="native"
RELEASE=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --wasm)
            BUILD_TYPE="wasm"
            shift
            ;;
        --native)
            BUILD_TYPE="native"
            shift
            ;;
        --release)
            RELEASE=true
            shift
            ;;
        --help)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --native     Build native binary (default)"
            echo "  --wasm       Build WebAssembly package"
            echo "  --release    Build in release mode"
            echo "  --help       Show this help message"
            exit 0
            ;;
        *)
            print_error "Unknown option: $1"
            exit 1
            ;;
    esac
done

if [ "$BUILD_TYPE" = "wasm" ]; then
    print_status "Building WebAssembly package..."
    
    # Check for wasm-pack
    if ! command -v wasm-pack &> /dev/null; then
        print_error "wasm-pack not found. Install with: cargo install wasm-pack"
        exit 1
    fi
    
    WASM_ARGS="--target web --out-dir ../../web/pkg"
    
    if [ "$RELEASE" = true ]; then
        WASM_ARGS="$WASM_ARGS --release"
    else
        WASM_ARGS="$WASM_ARGS --dev"
    fi
    
    cd crates/drafftink-app
    wasm-pack build $WASM_ARGS --no-default-features
    cd ../..
    
    print_status "WASM build complete. Output in web/pkg/"
    print_status "To run: cd web && python3 -m http.server 8080"
    
elif [ "$BUILD_TYPE" = "native" ]; then
    print_status "Building native binary..."
    
    CARGO_ARGS=""
    if [ "$RELEASE" = true ]; then
        CARGO_ARGS="--release"
    fi
    
    cargo build $CARGO_ARGS
    
    print_status "Native build complete."
    
    if [ "$RELEASE" = true ]; then
        print_status "Binary: target/release/drafftink"
    else
        print_status "Binary: target/debug/drafftink"
    fi
fi
