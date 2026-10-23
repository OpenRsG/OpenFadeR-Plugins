#!/bin/sh
# Builds the plugin workspace and copies each library next to its plugin.json. Extra arguments go to cargo (e.g. --quiet).
# CARGO_BUILD_TARGET=<triple> cross-compiles (the libraries are then taken from target/<triple>/release).
set -e
cd "$(dirname "$0")"
[ -n "${CARGO_BUILD_TARGET:-}" ] || unset CARGO_BUILD_TARGET # CI passes an empty one for a native build
case "$(uname -s)" in
Linux) os=linux ;;
Darwin) os=macos ;;
MINGW* | MSYS* | CYGWIN*) os=windows ;;
*) echo "unsupported OS: $(uname -s)" >&2; exit 1 ;;
esac
if [ "$os" = windows ]; then
	# The SQLCipher plugins build OpenSSL from source: with Git Bash's own perl (too few modules) that fails, so use Strawberry Perl
	# (on the GitHub runners) and NASM when they are there.
	[ -x /c/Strawberry/perl/bin/perl.exe ] && export OPENSSL_SRC_PERL=C:/Strawberry/perl/bin/perl.exe
	[ -d "/c/Program Files/NASM" ] && PATH="$PATH:/c/Program Files/NASM"
fi
cargo build --release --manifest-path Cargo.toml "$@"
out=target/release
[ -n "${CARGO_BUILD_TARGET:-}" ] && out=target/$CARGO_BUILD_TARGET/release
py=$(command -v python3 || command -v python)
for manifest in */plugin.json; do
	dir=$(dirname "$manifest")
	lib=$("$py" -c 'import json,sys; d=json.load(open(sys.argv[1])); print((d.get("library") or d["controller"])["lib"][sys.argv[2]])' "$manifest" "$os" 2>/dev/null) || continue
	cp "$out/$lib" "$dir/$lib"
done
