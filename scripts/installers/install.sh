#!/bin/sh
set -eu
payload=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
prefix=${JYNX_PREFIX:-"$HOME/.local/share/jynx"}
bin_dir=${JYNX_BIN_DIR:-"$HOME/.local/bin"}
show_privacy=0
while [ "$#" -gt 0 ]; do
 case "$1" in
  --privacy) show_privacy=1; shift ;;
  --prefix) prefix=$2; shift 2 ;;
  --bin-dir) bin_dir=$2; shift 2 ;;
  *) printf 'Unknown option: %s\n' "$1" >&2; exit 1 ;;
 esac
done
if [ "$show_privacy" = 1 ]; then
 cat "$payload/PRIVACY.txt"
 exit 0
fi
printf '\nJynx privacy policy: no built-in analytics or automatic chat uploads to R3F Studio.\nOptional providers, web tools, updates, and downloads can contact external services.\n'
if [ -t 0 ] && [ -t 1 ]; then
 printf 'Press Enter to read the full policy before installation (Ctrl+C cancels). '
 IFS= read -r privacy_answer || exit 1
 if command -v more >/dev/null 2>&1; then more "$payload/PRIVACY.txt"; else cat "$payload/PRIVACY.txt"; fi
else
 printf 'No interactive prompt in this session. Read the included PRIVACY.txt or run this installer with --privacy.\n'
fi
printf 'Policy: https://github.com/R3FStudio/Jynx/blob/main/PRIVACY.md\nA readable copy will be kept at %s/PRIVACY.txt\n\n' "$prefix"
case "$prefix" in /*) ;; *) printf 'Use an absolute installation prefix.\n' >&2; exit 1 ;; esac
case "$prefix" in /|/usr|/usr/local|/usr/local/bin|/bin|/home|"$HOME") printf 'Refusing unsafe installation prefix.\n' >&2; exit 1 ;; esac
case "$bin_dir" in /*) ;; *) printf 'Use an absolute bin directory.\n' >&2; exit 1 ;; esac
mkdir -p "$(dirname -- "$prefix")" "$bin_dir"
if [ -e "$prefix" ] && [ ! -f "$prefix/install-manifest.json" ]; then
 printf 'Existing directory is not a Jynx installation: %s\n' "$prefix" >&2; exit 1
fi
for name in jynx-cli jynx-harness jynx-llama; do
 if [ -e "$bin_dir/$name" ] || [ -L "$bin_dir/$name" ]; then
  if [ ! -L "$bin_dir/$name" ] || [ "$(readlink "$bin_dir/$name")" != "$prefix/bin/$name" ]; then
   printf 'Refusing to replace another command: %s\n' "$bin_dir/$name" >&2; exit 1
  fi
 fi
done
stage=$(mktemp -d "$(dirname -- "$prefix")/.jynx-install.XXXXXX")
backup="${stage}.previous"
cleanup() { [ ! -d "$stage" ] || rm -rf -- "$stage"; }
trap cleanup EXIT HUP INT TERM
if [ -d "$prefix" ]; then cp -R "$prefix/." "$stage/"; fi
cp -R "$payload/." "$stage/"
chmod 755 "$stage/bin/jynx-cli" "$stage/bin/jynx-harness" "$stage/bin/jynx-llama" "$stage/uninstall.sh"
printf '%s\n' "$bin_dir" > "$stage/.installed-bin-dir"
if [ -d "$prefix" ]; then mv -- "$prefix" "$backup"; fi
if ! mv -- "$stage" "$prefix"; then
 [ ! -d "$backup" ] || mv -- "$backup" "$prefix"
 exit 1
fi
for name in jynx-cli jynx-harness jynx-llama; do ln -sfn "$prefix/bin/$name" "$bin_dir/$name"; done
[ ! -d "$backup" ] || rm -rf -- "$backup"
printf 'Jynx installed in %s\nCommands: %s\nOpen a terminal with that directory on PATH.\n' "$prefix" "$bin_dir"
