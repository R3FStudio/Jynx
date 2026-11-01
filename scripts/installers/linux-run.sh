#!/bin/sh
set -eu
expected='@ARCH@'
actual=$(uname -m)
case "$expected:$actual" in x64:x86_64|arm64:aarch64|arm64:arm64) ;; *) echo 'This installer is for @ARCH@.' >&2; exit 1 ;; esac
[ "$(uname -s)" = Linux ] || { echo 'Use the macOS or Windows installer.' >&2; exit 1; }
tmp=$(mktemp -d "${TMPDIR:-/tmp}/jynx-setup.XXXXXX")
trap 'rm -rf -- "$tmp"' EXIT HUP INT TERM
line=$(awk '/^__JYNX_PAYLOAD__$/{print NR+1;exit}' "$0")
[ -n "$line" ] || { echo 'Missing installer payload.' >&2; exit 1; }
tail -n "+$line" "$0" | tar -xz -C "$tmp"
sh "$tmp/install.sh" "$@"
exit 0
__JYNX_PAYLOAD__
