#!/usr/bin/env bash

set -u

TEST_TARGET="target-wasm-test"

cleanup() {
    rm -rf "$TEST_TARGET"
}

trap cleanup EXIT INT TERM HUP

case "${1:-}" in
    node)
        CARGO_TARGET_DIR="$TEST_TARGET" wasm-pack test --node
        ;;
    chrome)
        CARGO_TARGET_DIR="$TEST_TARGET" wasm-pack test --headless --chrome
        ;;
    firefox)
        CARGO_TARGET_DIR="$TEST_TARGET" \
            xvfb-run -a wasm-pack test \
            --headless \
            --firefox \
            --geckodriver ~/.cache/.wasm-pack/geckodriver-*/geckodriver
        ;;
    *)
        echo "Usage: $0 {node|chrome|firefox}"
        exit 1
        ;;
esac