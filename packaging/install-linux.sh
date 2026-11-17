#!/usr/bin/env bash
# Installe la version prête à l'emploi de StudentCode pour l'utilisateur courant
# (sans Python ni bibliothèques à installer) : ./install-linux.sh
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEST="$HOME/.local/share/studentcode-app"
BIN="$HOME/.local/bin"
APPS="$HOME/.local/share/applications"
ICONS="$HOME/.local/share/icons/hicolor/scalable/apps"
mkdir -p "$BIN" "$APPS" "$ICONS"
rm -rf "$DEST"
cp -r "$HERE/StudentCode" "$DEST"
cat > "$BIN/studentcode" <<EOF
#!/usr/bin/env bash
exec "$DEST/StudentCode" "\$@"
EOF
chmod +x "$BIN/studentcode"
cp "$DEST/_internal/icon.svg" "$ICONS/studentcode.svg"
cat > "$APPS/studentcode.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=StudentCode
Comment=Code, cours et labs au même endroit
Exec=$BIN/studentcode %F
Icon=studentcode
Terminal=false
Categories=Development;IDE;Education;
StartupWMClass=studentcode
EOF
update-desktop-database "$APPS" >/dev/null 2>&1 || true
gtk-update-icon-cache -q "$HOME/.local/share/icons/hicolor" >/dev/null 2>&1 || true
# bibliothèque système demandée par Qt pour afficher les fenêtres sous X11
if ! ldconfig -p 2>/dev/null | grep -q libxcb-cursor.so.0; then
  echo "À installer une fois : sudo apt install libxcb-cursor0"
fi
echo "StudentCode est installé : lance-le depuis le menu des applications ou avec « studentcode »."
