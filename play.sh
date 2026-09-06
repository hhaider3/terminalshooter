#!/bin/sh
set -eu
cd "$(dirname "$0")"
if [ -x .tools/cargo/bin/cargo ]; then
    export RUSTUP_HOME="$PWD/.tools/rustup"
    export CARGO_HOME="$PWD/.tools/cargo"
    export PATH="$CARGO_HOME/bin:$PATH"
fi
if ! command -v cargo >/dev/null 2>&1; then
    printf '%s\n' 'Rust is required to build. Install it from https://rustup.rs, then run ./play.sh again.' >&2
    exit 1
fi
cargo build --release --locked
exec target/release/terminalshooter "$@"
