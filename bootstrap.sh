#!/usr/bin/env bash
# Build the vminit workspace (init + shell crates).
# bin/ is Linux/musl-only; build it inside WSL2 with:
#   cd main/features/bin && cargo build --target x86_64-unknown-linux-musl --release
set -e
cargo build --workspace "$@"
