# StudentCode (version Rust)

Réécriture en Rust de StudentCode, pensée pour coûter le moins possible : une interface
native dessinée par la carte graphique, sans moteur web ni Python au démarrage.

## Installation

**Windows** : `StudentCode-<version>-windows-x64-setup.exe`. L'installateur propose
« Complète » (PyScripter et lecteur PDF), « Minimale » (éditeur seul) ou « Personnalisée », et
ajoute la commande `studentcode` aux terminaux.

**Linux** :

```bash
tar xzf StudentCode-*-linux-x86_64.tar.gz && cd StudentCode-*-linux-x86_64
./install.sh            # demande si on installe le module PyScripter
studentcode .
```

`./install.sh --all` installe tout sans question, `./install.sh --minimal` seulement l'éditeur.
Les mots de passe retenus vont dans le trousseau du bureau si `secret-tool` est installé
(`sudo apt install libsecret-tools`), sinon dans un fichier lisible par toi seul.
Si la version Python est installée, elle reste disponible sous le nom `studentcode-py`.

## Personnaliser

**Paramètres** (Ctrl+, ou menu ☰ → Affichage → Paramètres…) :

- **Apparence** : thème, taille de toute l'interface (75 à 250 %), taille du code, police du
  code et de l'interface (n'importe quelle police installée ou un fichier .ttf/.otf), barre d'état.
- **Disposition** : panneau Fichiers/SSH/VPN à gauche ou à droite, Sortie et terminaux en bas ou à
  droite, débogage à droite ou à gauche ; ordre des onglets ; boutons de la barre d'outils
  (affichés ou non, ordre, séparateurs).
- **Éditeur** : largeur des tabulations, espaces ou tabulations, indentation automatique,
  fermeture des parenthèses et guillemets, numéros de ligne, pliage, guides, ligne du curseur,
  parenthèse correspondante, retour à la ligne automatique, enregistrement automatique, espaces
  en fin de ligne.
- **Terminal et exécution** : shell, taille du texte, lignes gardées, interpréteur Python (un
  environnement virtuel par exemple), enregistrement avant l'exécution, Sortie effacée ou non.
- **Raccourcis clavier** : chaque action se règle (clic, puis les touches) ; les doublons sont
  signalés en rouge.

Tout se déplace aussi à la souris : glisser un onglet de fichier pour changer l'ordre, vers un
autre volet ou vers le bord d'un volet pour le scinder ; glisser les onglets des panneaux ; clic
droit sur les onglets d'un panneau pour le changer de côté ; clic droit sur la barre d'outils pour
choisir les boutons. Les réglages sont dans `settings.json` (dossier de configuration de
StudentCode).

## Modules

| Module | Contenu | Coût |
|---|---|---|
| Noyau (toujours) | éditeur, écran partagé, explorateur, terminal, exécution, SSH et labs, VPN, images, favoris web | ~40 à 60 Mo |
| PyScripter (facultatif) | débogueur pas à pas, variables, console Python après exécution, bibliothèques Python | 0 tant qu'on ne s'en sert pas |
| Lecteur PDF (facultatif) | PDF dans un onglet (pdfium, ~6 Mo sur le disque) | chargé à l'ouverture du premier PDF |

Un module non installé n'est ni chargé ni affiché.

## Développement

```bash
cd rust
cargo run -- ../un_dossier          # lancer
cargo test                          # tests
STUDENTCODE_MODULES=modules cargo run   # avec le module PyScripter du dépôt
```

Les versions sont fabriquées par `.github/workflows/release-rust.yml` à chaque étiquette
`rust-v<version>` (envoyée par `publier.sh` quand la version de `Cargo.toml` change).
