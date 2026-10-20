#!/bin/sh
# Cross-build the single-file Windows exe (assets embedded).
# Needs: brew install mingw-w64 && rustup target add x86_64-pc-windows-gnu
set -eu
cargo build --release --target x86_64-pc-windows-gnu --bin operation-purge "$@"
echo target/x86_64-pc-windows-gnu/release/operation-purge.exe
