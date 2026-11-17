# StudentCode

Un espace de travail pour étudiant en informatique sur Ubuntu : un IDE Python façon
PyScripter, dans une interface à volets inspirée de VS Code et de LazyVim, où le code,
les pages de cours et les PDF tiennent dans la même fenêtre.

![Le terminal intégré sous l'éditeur](docs/terminal.png)

## Installation

Les fichiers sont sur la page **Releases** du dépôt.

### Windows 10 et 11

Télécharge `StudentCode-<version>-windows-x64-setup.exe` et lance-le (pas besoin d'être
administrateur). Ou prends le `.zip` portable : décompresse-le et lance `StudentCode.exe`.

L'app contient déjà tout ce qu'il lui faut, sauf les langages eux-mêmes :

- **Python** (pour exécuter ton code, la console et le débogueur) :
  [python.org](https://www.python.org/downloads/), en cochant « Add python.exe to PATH » ;
- **C / C++** : MinGW-w64, par exemple avec [MSYS2](https://www.msys2.org) (`pacman -S mingw-w64-ucrt-x86_64-gcc`),
  puis ajoute `C:\msys64\ucrt64\bin` au PATH ;
- **Java** : un JDK, par exemple [Temurin](https://adoptium.net) ;
- **VPN** : [OpenVPN Community](https://openvpn.net/community-downloads/) (OpenVPN GUI).

Windows peut afficher « Windows a protégé votre ordinateur » (l'app n'est pas signée) :
« Informations complémentaires » → « Exécuter quand même ».

### Linux, version prête à l'emploi

```bash
tar xzf StudentCode-*-linux-x86_64.tar.gz && cd StudentCode-*-linux-x86_64/
./install-linux.sh
studentcode
```

Rien d'autre à installer (sauf parfois `sudo apt install libxcb-cursor0`). Le Python de
l'ordinateur (`python3`) sert à exécuter ton code.

### Linux, depuis les sources (Ubuntu 24.04 et plus récent)

```bash
tar xzf StudentCode-*-source.tar.gz && cd StudentCode-*/
./install.sh            # ou ./install.sh --no-libs pour une mise à jour rapide
~/.local/bin/studentcode
```

Ou depuis le dépôt : `git clone` puis `./install.sh` dans le dossier. Tout est installé dans
`~/.local/share/studentcode`, avec un environnement Python dédié ; rien n'est modifié dans le
système, à part quelques paquets apt demandés au premier lancement de l'installeur. Pour
désinstaller :

```bash
rm -rf ~/.local/share/studentcode ~/.local/bin/studentcode ~/.local/share/applications/studentcode.desktop
```

## Volets

- **Scinder** : Alt+V (à droite), Alt+H (en bas), ou les boutons de la barre. Chaque volet a ses onglets.
- **Dans un onglet** : du code, une page web (Ctrl+T), un PDF, une image.
- **Ouvrir à côté** : clic droit sur un fichier de l'explorateur, ou sur un onglet pour le déplacer.
- **Naviguer** : Alt+W volet suivant, Alt+Maj+W déplacer l'onglet, Alt+Q fermer le volet.
- **Web** : barre d'adresse, favoris (étoile), session conservée entre deux lancements,
  PDF du site affichés dans l'onglet, téléchargements dans le dossier Téléchargements.
- F5 et le débogueur visent toujours le dernier fichier de code utilisé, même si le
  curseur est dans la page web.

## Aide par IA

- Bouton **IA** en haut à droite (Alt+I) : ouvre le panneau d'aide à droite de la fenêtre.
- **Par défaut, ton compte** : ChatGPT, Claude, Gemini ou Mistral s'affiche dans le panneau. Le bouton
  « Se connecter » (ou un « Continuer avec Google ») ouvre la connexion dans un grand onglet ; une fois
  connecté, l'onglet se ferme et la discussion revient dans le panneau. La session est conservée.
- Une action rapide (expliquer, trouver l'erreur, relire, proposer des tests) ou ta question est collée dans la
  discussion avec le pré-prompt de tuteur, le fichier ouvert (ou ta sélection) et la sortie du programme pour une
  erreur ; tu envoies toi-même.
- **Autres connexions** (la clé jaune du panneau) : clé d'API OpenAI, Anthropic, Gemini ou Mistral, modèle local
  Ollama, ou service compatible OpenAI. La réponse s'affiche alors directement dans le panneau, et la clé est
  enregistrée (trousseau du système, sinon fichier privé).
- Le pré-prompt est modifiable. Une IA peut se tromper : vérifie en exécutant ton code.

## Terminal

- Onglet **Terminal** du panneau du bas (Alt+T) : ton shell, avec ton invite, tes couleurs et tes alias.
- Un vrai terminal : vim, nano, htop, complétion par Tab, Ctrl+C, historique (Maj+PagePréc.).
- Ctrl+Maj+C / Ctrl+Maj+V pour copier-coller ; le clic du milieu colle la sélection.
- L'onglet **Terminal SSH** est le même terminal, sur la machine du lab connecté.
- Sous Windows, le terminal local est PowerShell.

## Lecteur PDF

- Les PDF s'ouvrent dans un lecteur intégré qui laisse toute la place à la page : ajusté à la largeur,
  défilement continu, et une barre d'outils flottante (page, zoom, page entière, mode sombre, recherche)
  qui n'apparaît que quand la souris bouge.
- Ctrl+molette ou Ctrl+/− pour zoomer, Ctrl+0 pour revenir à la largeur, Ctrl+F pour chercher
  (Entrée : suivant, Maj+Entrée : précédent).
- Sélection du texte à la souris (double-clic : un mot), Ctrl+C pour copier ; les liens sont cliquables.
- La page lue est retenue : le PDF se rouvre là où tu l'avais laissé.
- Un PDF ouvert depuis un site (Moodle…) est téléchargé puis affiché dans ce lecteur.

## Langages

Coloration aux couleurs du thème pour une soixantaine de types de fichiers : Python, C/C++, Java,
Gradle/Groovy, Kotlin, C#, Rust, Go, Swift, Dart, Scala, JavaScript/TypeScript, HTML/PHP/Vue, XML,
CSS/SCSS, JSON, YAML, TOML/.properties/.ini, Markdown, SQL, shell, Batch, Dockerfile, Makefile, CMake,
Lua, Perl, Ruby, LaTeX, Matlab/Octave, Pascal, Fortran, VHDL/Verilog, assembleur, diff… Les fichiers
sans extension connus (`Makefile`, `Dockerfile`, `gradlew`, `CMakeLists.txt`…) sont reconnus à leur nom.
Ctrl+/ commente avec la bonne syntaxe (`//`, `#`, `--`, `<!-- -->`, `/* */`…), et la complétion propose
les mots-clés du langage. Complétion intelligente et débogueur pas à pas : Python seulement.

## Thèmes et disposition

- ☰ → Affichage → Thème : Tokyo Night, Storm, Catppuccin, Everforest, Gruvbox, Nord, Dracula,
  Rosé Pine et trois thèmes clairs. L'application se relance et rouvre ta session.
- Glisse un onglet vers le bord droit, gauche ou bas d'un volet pour scinder de ce côté ;
  vers un autre volet pour l'y déplacer.
- Barres de défilement masquées par défaut (la molette fait défiler) ; ☰ → Affichage → « Barres de
  défilement » pour les afficher.

## VPN

- Onglet **VPN** du panneau de gauche : « Ajouter un VPN (.ovpn) », puis « Connecter ».
- Identifiant et mot de passe demandés une fois, gardés dans le trousseau du système.
- Passe par NetworkManager : le VPN ajouté apparaît aussi dans le menu réseau d'Ubuntu.
- **Internet conservé** : la case « Garder internet : VPN seulement pour le lab » (cochée par défaut) ne fait
  passer par le VPN que le réseau du lab ; le reste passe par ta connexion normale. La machine SSH à laquelle
  tu te connectes est ajoutée toute seule ; d'autres réseaux se saisissent dans le champ en dessous.
- Un lab SSH peut indiquer son VPN (« VPN à activer avant ») : il est monté tout seul à la connexion.
- **Sous Windows**, l'onglet passe par OpenVPN GUI : le fichier est rangé dans `~/OpenVPN/config`,
  l'identifiant se saisit dans la fenêtre d'OpenVPN, et « Garder internet » fonctionne pareil.

## Session

À la fermeture, StudentCode retient les volets, leurs tailles, les onglets ouverts (code, PDF,
images, pages web) et la position du curseur ; tout est rouvert à l'identique au lancement suivant.
Les fichiers ouverts depuis un lab SSH ne sont pas rouverts (il faut d'abord se reconnecter).

## SSH (labs, RemoteLabz…)

- **Connexion** : onglet SSH du panneau de gauche (ou Ctrl+Alt+S). Tape `ubuntu@adresse-ip` puis Entrée,
  saisis le mot de passe, c'est connecté. Pour nommer la machine : `Serveur = ubuntu@adresse-ip`, ou
  menu « … » → « Renommer cette machine ».
- **Plusieurs machines en même temps** : le bouton « + » de l'onglet SSH ouvre une autre connexion. Chaque
  machine a son sous-onglet, son explorateur et son terminal (« SSH Serveur », « SSH Croupier »…) ; les
  onglets de fichiers indiquent leur machine (« Serveur ⇄ Carte.java »).
- **Déplacer** : glisse un fichier distant sur un dossier pour le déplacer ; glisse un fichier de
  l'explorateur local vers l'explorateur distant pour l'envoyer.
- **Options avancées** (menu « … ») : port, clé SSH, VPN à activer avant, machine relais, nettoyage.
  On peut aussi coller une commande `ssh -p 2222 user@machine` dans le champ.
- **Mot de passe** : gardé dans le trousseau du système (jamais en clair dans un fichier).
- **Explorateur distant** : ouvrir, créer, renommer, supprimer, envoyer et télécharger des fichiers.
- **Édition** : un fichier distant s'ouvre dans un onglet (marqué ⇄) ; Ctrl+S l'envoie sur la machine.
- **F5** exécute le fichier sur la machine (sortie et `input()` dans l'onglet Sortie).
- **Terminal SSH** dans le panneau du bas : un vrai terminal sur la machine (vim, nano, Tab).
- **Nettoyage à la connexion** (automatique) : avant d'ouvrir la session, StudentCode ferme ce qui traîne
  d'une séance précédente : tes anciennes sessions SSH, tes programmes encore en écoute sur un port (le
  serveur du TP qui bloque avec « Address already in use »), et tes programmes java (dont les démons Gradle),
  python, node… restés en arrière-plan. Jamais les services du système ni la connexion en cours. Pour le
  désactiver sur une machine (compte partagé, programme à laisser tourner) : menu « … » → Options avancées.
- Une seule connexion par lab ; une coupure est détectée et signalée, rien n'est perdu.

## Complétion

Automatique pendant la frappe, y compris pour les classes et fonctions de tes autres fichiers
(`from Message import Message` puis `m.` propose ses méthodes). Ctrl+Espace la force, une
info-bulle montre les paramètres après `(`, F12 va à la définition.

## Côté Python (l'ancien LazyPy)

- **Éditeur** : coloration, pliage, indentation auto, complétion (jedi), rechercher/remplacer.
- **C, C++ et Java** : F5 compile puis lance (gcc/g++ ; pour Java, tous les `.java` du dossier sont compilés
  et la classe du fichier courant est lancée). Il faut `gcc` ou un JDK installés.
- **Exécution** (F5) ; après la fin du programme, l'interpréteur reste actif sur ses
  variables, fonctions et classes (ex. `Message([2, 3])`).
- **Débogueur pas à pas** : points d'arrêt (F9 ou clic dans la marge), continuer (F6),
  pas à pas (F10), entrer (F11), sortir (Maj+F11), variables dépliables, pile d'appels.
- **Bibliothèques** (Ctrl+Maj+L) : installation, mise à jour, désinstallation via pip.
- **Explorateur** : sélection multiple, glisser-déposer, renommer (F2), corbeille (Suppr).
- **Ctrl+P** recherche floue de fichiers · **F1** tous les raccourcis · mode Vim optionnel.

## Limites connues

- Le mode Vim couvre l'essentiel, pas tout Vim.
- Le débogueur suit le thread principal et ne s'arrête que dans ton code.
- Complétion intelligente (jedi) et débogueur pas à pas pour Python seulement ; ailleurs, complétion sur les mots du fichier.
- Le débogueur pas à pas ne fonctionne que sur les fichiers locaux.
- La version Windows est construite et vérifiée automatiquement, mais moins éprouvée au quotidien
  que la version Linux ; l'app n'est pas signée (avertissement SmartScreen au premier lancement).

## Développement

- Le code est dans `studentcode/` (Python, PyQt6) ; `python3 studentcode/main.py` le lance sans installer,
  une fois les dépendances présentes (`pip install -r packaging/requirements.txt`).
- `./scripts/make-release.sh` construit l'archive des sources dans `dist/`.
- `pyinstaller packaging/studentcode.spec` construit l'app prête à l'emploi (Windows ou Linux, selon
  la machine) dans `dist/StudentCode/` ; `dist/StudentCode/StudentCode --selftest` la vérifie.
- La fabrication automatique (`.github/workflows/release.yml`) construit et vérifie Windows et
  Linux ; lancée à la main depuis l'onglet Actions, elle ne publie rien.
- Pour publier une version : changer `__version__` dans `studentcode/main.py`, compléter `CHANGELOG.md`,
  puis `./publier.sh` (ou pousser une étiquette `vX.Y.Z` : la publication se fait toute seule).
