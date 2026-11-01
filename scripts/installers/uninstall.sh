#!/bin/sh
set -eu
prefix=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
[ -f "$prefix/install-manifest.json" ] || { echo 'Not a Jynx installation.' >&2; exit 1; }
case "$prefix" in /|/usr|/usr/local|/usr/local/bin|/bin|/home|"$HOME") echo 'Refusing unsafe uninstall path.' >&2; exit 1 ;; esac
bin_dir=$(cat "$prefix/.installed-bin-dir" 2>/dev/null || printf '%s' /usr/local/bin)
for name in jynx-cli jynx-harness jynx-llama; do
 if [ -L "$bin_dir/$name" ] && [ "$(readlink "$bin_dir/$name")" = "$prefix/bin/$name" ]; then rm -- "$bin_dir/$name"; fi
done

rm -f -- "$prefix/bin/jynx-cli" "$prefix/bin/jynx-harness" "$prefix/bin/jynx-llama"
rmdir "$prefix/bin" 2>/dev/null || true
rm -rf -- "$prefix/runtimes" "$prefix/licenses" "$prefix/setup" "$prefix/source"
rm -f -- "$prefix/LICENSE" "$prefix/COPYRIGHT" "$prefix/PRIVACY.md" "$prefix/PRIVACY.txt" "$prefix/README.txt" "$prefix/runtime-manifest.json" "$prefix/install-manifest.json" "$prefix/.installed-bin-dir" "$prefix/install.sh" "$prefix/uninstall.sh"
rmdir "$prefix" 2>/dev/null || true
printf 'Jynx removed. Model and conversation data were kept.\n'
