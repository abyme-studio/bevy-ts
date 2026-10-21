#!/bin/sh
# cargo's rustc-wrapper (.cargo/config.toml): sccache when installed, else rustc as is.
if command -v sccache >/dev/null 2>&1; then exec sccache "$@"; fi
exec "$@"
