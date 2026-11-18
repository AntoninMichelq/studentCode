#!/usr/bin/env bash
# Met StudentCode sur ton GitHub et publie la version.
#   ./publier.sh                       dépôt « studentCode » de ton compte (créé s'il n'existe pas)
#   ./publier.sh mon-depot             autre nom ; une adresse github.com/... marche aussi
# Si le dépôt existe déjà avec du contenu, il est fusionné : rien n'est écrasé.
# Les commits sont faits avec TON identité git ; rien n'est envoyé sans ton accord.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"
REPO="${1:-studentCode}"
REPO="${REPO#https://github.com/}"; REPO="${REPO%.git}"; REPO="${REPO%/}"
VERSION="$(sed -n 's/^__version__ = "\(.*\)"/\1/p' studentcode/main.py)"
TAG="v$VERSION"

command -v git >/dev/null || { echo "git manque : sudo apt install git"; exit 1; }
if ! git config user.name >/dev/null || ! git config user.email >/dev/null; then
    echo "Git ne connaît pas encore ton identité. Lance d'abord :"
    echo '    git config --global user.name  "Ton Nom"'
    echo '    git config --global user.email "ton@email"'
    exit 1
fi

if [ ! -d .git ]; then
    git init -q -b main
fi
git add -A
if ! git diff --cached --quiet; then
    git commit -q -m "StudentCode $VERSION"
    echo "==> Commit créé ($(git log -1 --format=%h))"
fi

if ! command -v gh >/dev/null || ! gh auth status >/dev/null 2>&1; then
    cat <<EOF

Le dépôt local est prêt. Pour l'envoyer, deux possibilités :

 A) Avec l'outil GitHub (le plus simple) :
      sudo apt install gh && gh auth login
      ./publier.sh $REPO

 B) À la main : crée un dépôt vide « $REPO » sur github.com, puis
      git remote add origin https://github.com/TON-PSEUDO/$REPO.git
      git push -u origin main
      git tag $TAG && git push origin $TAG
    L'étiquette déclenche la publication automatique de la version
    (onglet « Actions » du dépôt), avec l'archive en pièce jointe.
EOF
    exit 0
fi

BRANCH=main
if ! git remote get-url origin >/dev/null 2>&1; then
    if URL="$(gh repo view "$REPO" --json url -q .url 2>/dev/null)" && [ -n "$URL" ]; then
        echo "==> Le dépôt existe déjà : $URL"
        git remote add origin "$URL.git"
    else
        read -r -p "Dépôt public (tout le monde peut le voir) ? [o/N] " rep
        case "$rep" in o|O|oui|y|Y) VIS=--public ;; *) VIS=--private ;; esac
        echo "==> Création du dépôt $REPO ($VIS)"
        gh repo create "$REPO" "$VIS" --source=. --remote=origin \
            --description "Espace de travail pour étudiant en informatique : IDE Python, volets, SSH, VPN, aide IA"
    fi
fi
echo "==> Envoi vers $(git remote get-url origin)"
git fetch -q origin 2>/dev/null || true
# branche principale du dépôt distant (main, master…) s'il a déjà du contenu
REMOTE_HEAD="$(git ls-remote --symref origin HEAD 2>/dev/null | sed -n 's#^ref: refs/heads/\(.*\)\tHEAD#\1#p')"
if [ -n "$REMOTE_HEAD" ] && git rev-parse -q --verify "origin/$REMOTE_HEAD" >/dev/null; then
    BRANCH="$REMOTE_HEAD"
    [ "$(git branch --show-current)" = "$BRANCH" ] || git branch -M "$BRANCH"
    if ! git merge-base --is-ancestor "origin/$BRANCH" HEAD 2>/dev/null; then
        echo "    Le dépôt contient déjà des fichiers : fusion (les tiens sont gardés en cas de conflit)"
        git merge -q "origin/$BRANCH" --allow-unrelated-histories -X ours \
            -m "Fusion avec le contenu existant du dépôt"
    fi
fi
git push -u origin "$BRANCH"

if git rev-parse "$TAG" >/dev/null 2>&1; then
    echo "==> L'étiquette $TAG existe déjà (change __version__ dans studentcode/main.py pour une nouvelle version)"
else
    git tag "$TAG"
fi
git push origin "$TAG"
URL="$(gh repo view --json url -q .url)"
echo
echo "==> L'étiquette $TAG est envoyée : GitHub fabrique maintenant les versions Windows et Linux"
echo "    (environ 10 minutes), puis les publie sur $URL/releases/tag/$TAG"
echo "    Suivre la fabrication :  gh run watch -R ${URL#https://github.com/}"

# Version Rust : étiquette rust-v<version de rust/Cargo.toml>, fabriquée par release-rust.yml
if [ -f rust/Cargo.toml ]; then
    RVER="$(sed -n 's/^version = "\(.*\)"/\1/p' rust/Cargo.toml | head -1)"
    RTAG="rust-v$RVER"
    if git rev-parse "$RTAG" >/dev/null 2>&1; then
        echo "==> Version Rust : l'étiquette $RTAG existe déjà (change version dans rust/Cargo.toml pour une nouvelle version)"
    else
        git tag "$RTAG"
        git push origin "$RTAG"
        echo "==> Version Rust $RVER envoyée : publiée sur $URL/releases/tag/$RTAG (environ 10 minutes)"
    fi
fi
