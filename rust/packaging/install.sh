#!/usr/bin/env bash
# Installe SCode (ex-StudentCode, version Rust) pour l'utilisateur courant, sans droits administrateur.
#   ./install.sh                 demande quels modules installer
#   ./install.sh --all           tout installer sans poser de question
#   ./install.sh --minimal       seulement l'éditeur (sans module)
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEST="$HOME/.local/share/studentcode-rs"
BIN="$HOME/.local/bin"
APPS="$HOME/.local/share/applications"
ICONS="$HOME/.local/share/icons/hicolor/256x256/apps"
MODE="${1:-ask}"

ask() {  # ask "question" défaut(o/n)
    local q="$1" def="$2" r
    if [ "$MODE" = "--all" ]; then return 0; fi
    if [ "$MODE" = "--minimal" ]; then return 1; fi
    if [ ! -t 0 ]; then [ "$def" = "o" ]; return; fi
    read -r -p "$q [$( [ "$def" = o ] && echo O/n || echo o/N )] " r
    r="${r:-$def}"
    [[ "$r" =~ ^[oOyY] ]]
}

echo "==> SCode $(cat "$HERE/VERSION" 2>/dev/null) — installation dans $DEST"
echo
echo "Modules :"
echo "  • PyScripter : débogueur Python pas à pas, points d'arrêt, variables,"
echo "    console Python ouverte sur les variables après l'exécution."
echo "  • Lecteur PDF : sujets de TP et cours PDF dans un onglet, à côté du code"
echo "    (sinon les PDF s'ouvrent avec l'application habituelle)."
PYS=0; PDF=0
if ask "Installer le module PyScripter ?" o; then PYS=1; fi
if [ -f "$HERE/modules/pdf/libpdfium.so" ] && ask "Installer le module Lecteur PDF ?" o; then PDF=1; fi

mkdir -p "$DEST" "$BIN" "$APPS" "$ICONS"
install -m 755 "$HERE/studentcode" "$DEST/studentcode"
install -m 644 "$HERE/icon.png" "$DEST/icon.png"
rm -rf "$DEST/modules/pyscripter" "$DEST/modules/pdf"
if [ "$PDF" = 1 ]; then
    mkdir -p "$DEST/modules/pdf"
    cp "$HERE/modules/pdf/libpdfium.so" "$DEST/modules/pdf/"
fi
if [ "$PYS" = 1 ]; then
    mkdir -p "$DEST/modules/pyscripter"
    cp "$HERE/modules/pyscripter/"*.py "$DEST/modules/pyscripter/"
    command -v python3 >/dev/null || echo "   (Python n'est pas installé : sudo apt install python3)"
fi

# Commande « studentcode ». Si l'ancienne version (Python) est installée, elle reste
# disponible sous le nom « studentcode-py ».
if [ -f "$BIN/studentcode" ] && grep -q "share/studentcode/" "$BIN/studentcode" 2>/dev/null; then
    if ask "L'ancienne version (Python) est installée. La garder sous le nom « studentcode-py » ?" o; then
        mv "$BIN/studentcode" "$BIN/studentcode-py"
        [ -f "$APPS/studentcode.desktop" ] && sed -e 's/^Name=StudentCode$/Name=StudentCode (Python)/' \
            -e "s#$BIN/studentcode #$BIN/studentcode-py #" "$APPS/studentcode.desktop" > "$APPS/studentcode-py.desktop"
    fi
fi
cat > "$BIN/studentcode" <<EOF
#!/usr/bin/env bash
exec "$DEST/studentcode" "\$@"
EOF
chmod +x "$BIN/studentcode"
# nouveau nom de la commande
ln -sf "$BIN/studentcode" "$BIN/scode"
cp "$HERE/icon.png" "$ICONS/studentcode.png"
# icône au nom qui change avec son contenu : GNOME garde sinon l'ancienne en cache
rm -f "$DEST"/icon-*.png
ICON="$DEST/icon-$(md5sum "$HERE/icon.png" | cut -c1-8).png"
cp "$HERE/icon.png" "$ICON"
cat > "$APPS/studentcode.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=SCode
GenericName=Éditeur de code
Keywords=StudentCode;IDE;Python;code;
Comment=Éditeur de code léger pour étudiants
Exec=$BIN/studentcode %F
Icon=$ICON
Terminal=false
Categories=Development;IDE;Education;
StartupWMClass=studentcode
EOF
update-desktop-database "$APPS" >/dev/null 2>&1 || true
gtk-update-icon-cache -q "$HOME/.local/share/icons/hicolor" >/dev/null 2>&1 || true

echo
echo "Terminé. Lance SCode depuis le menu des applications, ou dans un terminal :"
echo "    scode .              (ouvre le dossier courant ; « studentcode » marche aussi)"
case ":$PATH:" in *":$BIN:"*) ;; *) echo "    (ajoute $BIN à ton PATH si la commande n'est pas trouvée)";; esac
[ "$PYS" = 1 ] && echo "Module PyScripter installé." || echo "Sans module PyScripter (relance ./install.sh pour l'ajouter)."
[ "$PDF" = 1 ] && echo "Module Lecteur PDF installé." || echo "Sans lecteur PDF intégré."
