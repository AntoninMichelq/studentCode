#!/usr/bin/env bash
# Installe StudentCode dans ~/.local/share/studentcode (aucun fichier système
# modifié, à part d'éventuels paquets apt manquants).
#   ./install.sh            installation complète
#   ./install.sh --no-libs  sans les bibliothèques par défaut (mise à jour rapide)
set -euo pipefail

SRC="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP="$HOME/.local/share/studentcode"
OLD="$HOME/.local/share/lazypy"
VENV="$APP/venv"
LIBS=1
[ "${1:-}" = "--no-libs" ] && LIBS=0

echo "==> Vérification des paquets système"
missing=()
for pkg in python3-venv python3-pip python3-tk libxcb-cursor0 libnss3 network-manager-openvpn-gnome; do
    dpkg -s "$pkg" >/dev/null 2>&1 || missing+=("$pkg")
done
if [ ${#missing[@]} -gt 0 ]; then
    echo "    Paquets à installer : ${missing[*]} (mot de passe sudo demandé)"
    sudo apt-get update -qq
    sudo apt-get install -y "${missing[@]}"
fi

mkdir -p "$APP"
if [ -d "$OLD/venv" ] && [ ! -d "$VENV" ]; then
    echo "==> Reprise de l'installation LazyPy (bibliothèques conservées)"
    mv "$OLD/venv" "$VENV"
    rm -rf "$OLD" "$HOME/.local/bin/lazypy" "$HOME/.local/share/applications/lazypy.desktop"
fi

echo "==> Copie de l'application dans $APP"
rm -rf "$APP/app"
cp -r "$SRC/studentcode" "$APP/app"

echo "==> Environnement Python ($VENV)"
python3 -m venv "$VENV"          # crée l'environnement, ou le répare s'il a été déplacé
"$VENV/bin/python" -m pip install --quiet --upgrade pip
"$VENV/bin/python" -m pip install --quiet PyQt6 PyQt6-QScintilla jedi
echo "==> Modules SSH, terminal et PDF (labs, trousseau, terminal intégré, lecteur PDF)"
"$VENV/bin/python" -m pip install --quiet paramiko keyring pyte pypdfium2 \
    || echo "    !! échec : installe-les plus tard depuis l'onglet SSH"
echo "==> Moteur web (pages de cours et PDF dans les onglets, ~150 Mo)"
"$VENV/bin/python" -m pip install --quiet PyQt6-WebEngine \
    || echo "    !! échec : StudentCode marchera, mais les pages web s'ouvriront dans ton navigateur"

if [ "$LIBS" = 1 ]; then
    echo "==> Bibliothèques par défaut (quelques minutes, ~1 Go)"
    grep -vE '^\s*(#|$)' "$SRC/bibliotheques.txt" | while read -r lib; do
        echo "    - $lib"
        "$VENV/bin/python" -m pip install --quiet "$lib" \
            || echo "      !! échec pour $lib (tu pourras réessayer depuis l'application)"
    done
fi

echo "==> Lanceur et entrée de menu"
mkdir -p "$HOME/.local/bin" "$HOME/.local/share/applications"
cat > "$HOME/.local/bin/studentcode" <<EOF
#!/usr/bin/env bash
exec "$VENV/bin/python" "$APP/app/main.py" "\$@"
EOF
chmod +x "$HOME/.local/bin/studentcode"
# L'icône est copiée sous un nom qui change avec son contenu : le bureau garde les
# icônes en mémoire par nom de fichier, donc un nouveau nom force son rechargement.
ICON="$APP/icon-$(md5sum "$APP/app/icon.svg" | cut -c1-8).svg"
rm -f "$APP"/icon-*.svg
cp "$APP/app/icon.svg" "$ICON"
mkdir -p "$HOME/.local/share/icons/hicolor/scalable/apps"
cp "$APP/app/icon.svg" "$HOME/.local/share/icons/hicolor/scalable/apps/studentcode.svg"
gtk-update-icon-cache -f -t "$HOME/.local/share/icons/hicolor" >/dev/null 2>&1 || true
cat > "$HOME/.local/share/applications/studentcode.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=StudentCode
Comment=Code, cours et PDF dans la même fenêtre
Exec=$HOME/.local/bin/studentcode %F
Icon=$ICON
Terminal=false
Categories=Development;IDE;Education;
MimeType=text/x-python;
StartupWMClass=studentcode
EOF
update-desktop-database "$HOME/.local/share/applications" >/dev/null 2>&1 || true

echo
echo "Terminé. Lance StudentCode depuis le menu des applications, ou avec :"
echo "    ~/.local/bin/studentcode"
echo "    ~/.local/bin/studentcode mon_dossier/    pour ouvrir un projet"
