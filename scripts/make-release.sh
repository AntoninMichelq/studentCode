#!/usr/bin/env bash
# Construit l'archive de la version : dist/StudentCode-<version>.tar.gz
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
VERSION="$(sed -n 's/^__version__ = "\(.*\)"/\1/p' studentcode/main.py)"
[ -n "$VERSION" ] || { echo "Version introuvable dans studentcode/main.py" >&2; exit 1; }
NAME="StudentCode-$VERSION"
rm -rf dist && mkdir -p "dist/$NAME"
cp -r studentcode install.sh bibliotheques.txt README.md CHANGELOG.md "dist/$NAME/"
[ -f LICENSE ] && cp LICENSE "dist/$NAME/"
find "dist/$NAME" -name __pycache__ -type d -prune -exec rm -rf {} +
tar -C dist -czf "dist/$NAME.tar.gz" "$NAME"
rm -rf "dist/$NAME"
echo "dist/$NAME.tar.gz"
