#!/bin/sh
# Builds the plugin workspace and copies each library next to its plugin.json. Extra arguments go to cargo (e.g. --quiet).
set -e
cd "$(dirname "$0")"
cargo build --release --manifest-path Cargo.toml "$@"
case "$(uname -s)" in
Linux) os=linux ;;
Darwin) os=macos ;;
MINGW* | MSYS* | CYGWIN*) os=windows ;;
*) echo "unsupported OS: $(uname -s)" >&2; exit 1 ;;
esac
for manifest in */plugin.json; do
	dir=$(dirname "$manifest")
	lib=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["library"]["lib"][sys.argv[2]])' "$manifest" "$os" 2>/dev/null) || continue
	cp "target/release/$lib" "$dir/$lib"
done
