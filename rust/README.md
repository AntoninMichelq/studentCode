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
