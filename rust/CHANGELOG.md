# StudentCode en Rust — journal des versions

## 2.1.1

- **Nouveau fichier** (Ctrl+N, bouton, menu, ␣fn) ne crée plus un .py d'office : une liste
  propose le type (Python, C, C++, Java, JavaScript, HTML, Rust, Makefile, texte…), le dernier
  choisi en premier. On peut aussi taper directement un nom (« main.c », « src/Test.java ») pour
  créer le fichier dans le dossier ouvert.
- « Enregistrer sous » propose le nom du fichier avec son extension.

## 2.1.0

**Mode Vim, activé par défaut, avec les raccourcis de LazyVim.**

- Modes Normal, Insertion, Visuel et Visuel-ligne, curseur bloc, mode affiché dans la barre
  d'état (NORMAL, INSERT, VISUAL, COMMAND), numéros de ligne relatifs.
- Mouvements (h j k l, w b e, 0 ^ $, gg G, f t ; ,, %, { }, Ctrl+d / Ctrl+u), opérateurs (d c y
  > < gc gu gU g~) avec compteurs et objets de texte (iw aw, i( a(, i{, i[, i" i', ip…),
  x s r J ~ p P, u / Ctrl+r, « . », recherche / ? n N * #, gcc pour commenter, Alt+j / Alt+k pour
  déplacer les lignes. Le copier-coller passe par le presse-papiers du système, comme LazyVim.
- Commandes « : » : :w :q :wq :x :q! :qa :wa :e fichier :e! :sp :vs :bn :bp :bd :enew :term
  :set (nu, rnu, wrap) :colorscheme, :12 pour aller à une ligne, :s/a/b/g, :%s/…/…/g.
- **Espace (leader)** ouvre la fenêtre des raccourcis, comme which-key : ␣␣ fichiers, ␣/ grep,
  ␣, onglets, ␣e explorateur, ␣- ␣| scinder, ␣ff ␣fr ␣fn ␣fc, ␣bd ␣bo, ␣d… débogage (␣db point
  d'arrêt, ␣dc continuer, ␣di ␣dO ␣do pas à pas, ␣dt arrêter), ␣u… interface (␣uw ␣ul ␣uL ␣uC
  ␣ub ␣uz), ␣w… volets, ␣gg lazygit, ␣qq quitter, ␣sk liste des raccourcis.
- **Harpoon** : ␣H marque le fichier, ␣1 à ␣9 y retournent, ␣h ouvre la liste.
- Recherche façon Telescope (fichiers, texte dans le projet, onglets, récents, thèmes).
- Hors leader : H / L onglet précédent / suivant, Ctrl+h/j/k/l changer de volet, Ctrl+/
  terminal (et retour au code), Ctrl+S enregistre dans tous les modes.
- Désactivable : Paramètres → Éditeur, menu Affichage, ou Ctrl+Alt+V.

**Aussi dans cette version** (la 2.0.4 n'a pas été publiée) :

- **StudentCode s'appelle maintenant SCode** : fenêtre, menus, installateurs, raccourcis du
  bureau et du menu Démarrer, fichiers à télécharger (`SCode-…`). Nouvelle commande `scode .`
  (`studentcode .` marche toujours). Les réglages, labs, mots de passe et sessions sont gardés,
  et sous Windows la mise à jour remplace l'ancienne installation et ses raccourcis.
- **Thèmes clairs** : le code s'affichait en morceaux de lettres (surtout sous Windows). Passer
  en thème clair change le rendu des lettres, et l'éditeur réutilisait l'ancienne mise en page.
  Corrigé pour les 11 thèmes, y compris en changeant de thème pendant le travail et quand
  l'image des lettres se remplit (gros fichiers, zoom).
- **Thème clair par défaut** (« Papier ») au premier lancement.
- **Maj+Tab** désindente bien (il indentait, comme Tab) ; Maj+Entrée dans la recherche va à
  l'occurrence précédente.
- **Ctrl+G** et les petites fenêtres (nouveau fichier, renommer, mot de passe…) prennent le
  clavier tout de suite : ce qu'on tapait partait dans le code et pouvait remplacer la sélection.
  Échap les ferme, et les raccourcis sont en pause tant qu'elles sont ouvertes.
- **Nouvelle icône** : un « S » vide, juste son contour, avec le curseur « _ » d’un terminal, sur une tuile papier encadrée. Elle est
  maintenant intégrée au programme sous Windows (explorateur, barre des tâches, raccourcis), et
  Ubuntu affiche la nouvelle sans garder l'ancienne en cache.

## 2.0.3

Revue complète de l'application.

- **Menu ☰** : le clic gauche n'ouvrait plus le menu (il se refermait aussitôt, à cause du
  menu « clic droit » de la barre d'outils). Même correction pour le bouton des pages web et le
  bouton « » » des fenêtres étroites.
- **Clics plus tolérants** : un clic pendant lequel la souris bouge de quelques pixels, ou un
  clic lent, compte bien comme un clic (boutons, menus, onglets, fichiers de l'explorateur).
- Le clavier va tout de suite dans l'éditeur : au lancement, après « Nouveau fichier », en
  ouvrant un fichier, en changeant ou en fermant un onglet.
- Menu Édition : Couper, Copier, Coller, Tout sélectionner agissent bien sur la sélection (elle
  était perdue en ouvrant le menu).
- Sortie : le message d'erreur d'un programme s'affiche avant « Programme terminé », plus
  parfois après.

## 2.0.2

- **Copier / coller avec les autres programmes** : Ctrl+C, Ctrl+X, Ctrl+V ne passaient pas par
  le presse-papiers du système (Windows comme Linux). C'est réparé partout : éditeur, champs,
  terminal.
- Éditeur : **clic droit** Annuler, Rétablir, Couper, Copier, Coller, Supprimer, Tout
  sélectionner, Commenter, Dupliquer ; les mêmes actions dans le menu Édition.
- Terminal : Ctrl+V et Ctrl+Maj+V collent, Ctrl+C copie la sélection (sinon interrompt le
  programme), Ctrl+Maj+C copie ; « Coller » du clic droit marche.
- Nouveaux raccourcis (modifiables) : Alt+↑ / Alt+↓ déplacer la ligne, Ctrl+Maj+K supprimer la
  ligne, F3 / Maj+F3 occurrence suivante / précédente, Ctrl+H remplacer ; Ctrl+F reprend le
  texte sélectionné.
- Les raccourcis qui modifient le texte n'agissent que dans l'éditeur (plus dans un champ de
  saisie).

## 2.0.1

- Windows : le terminal restait vide (PowerShell ne démarrait pas). La console de Windows
  demande la position du curseur au démarrage et attend la réponse ; StudentCode répond
  maintenant à cette question (et aux autres questions des programmes au terminal).
- L'autotest de la fabrication vérifie qu'un vrai shell répond dans le terminal, sous Windows
  comme sous Linux.

## 2.0.0

Première version stable de la réécriture en Rust (plus une préversion).

**Tout se personnalise** — nouvelle fenêtre **Paramètres** (Ctrl+,) :

- Apparence : thème, taille de l'interface (75 à 250 %), taille du code, polices du code et de
  l'interface (toute police installée ou un fichier), barre d'état affichée ou non.
- Disposition : panneau latéral à gauche ou à droite, panneau du bas en bas ou à droite, débogage
  à droite ou à gauche ; ordre des onglets ; barre d'outils au choix (boutons, ordre,
  séparateurs ; aussi par clic droit sur la barre).
- Éditeur : tabulations (largeur, espaces ou tabulation), indentation automatique, fermeture
  automatique de ( [ { " ', numéros de ligne, pliage, guides, ligne du curseur, parenthèse
  correspondante, **retour à la ligne automatique**, **enregistrement automatique**, espaces
  en fin de ligne enlevés.
- Terminal et exécution : shell, taille du texte, historique, **interpréteur Python au choix**
  (environnement virtuel), enregistrement avant l'exécution, Sortie effacée ou non.
- **Raccourcis clavier modifiables**, avec détection des doublons.

**Tout se déplace** :

- les onglets de fichiers se réordonnent en les glissant, et passent d'un volet à l'autre en les
  déposant dans sa barre d'onglets ;
- les onglets du panneau latéral et du panneau du bas se réordonnent aussi ;
- clic droit sur les onglets d'un panneau : le changer de côté, le masquer.

**Corrections** :

- les raccourcis comparent exactement les touches : Ctrl+Maj+S n'enregistre plus sans demander
  le nom (Ctrl+S déclenchait avant Ctrl+Maj+S) ; Alt+M ouvre le menu ;
- Ctrl+/ commente aussi sur un clavier AZERTY (Maj ignorée pour les signes comme / + :) ;
- la recherche sans tenir compte des majuscules sélectionnait le mauvais texte après certains
  caractères (ß, İ…), et « Remplacer tout » pouvait couper un caractère ;
- les points d'arrêt sont gardés aussi pour les fichiers rouverts plus tard ;
- les cases à cocher passent à la ligne dans un panneau étroit au lieu d'être coupées ;
- la barre d'onglets du bas défile quand le panneau est étroit.

## 2.0.0-alpha.4

Même apparence que la version Python, élément par élément (comparé capture contre capture) :

- **Icônes** : exactement celles de la version Python (mêmes dessins, générés par
  `scripts/gen_rust_icons.py`), y compris les pastilles de type de fichier (`py`, `jv`, `gr`…).
- **Barre d'outils** : mêmes boutons dans le même ordre (menu, nouveau, ouvrir, enregistrer,
  exécuter, déboguer, pas à pas, arrêter, point d'arrêt, bibliothèques, rechercher, terminal,
  volets, web, SSH, VPN), titre au centre ; dans une fenêtre étroite, les derniers boutons passent
  dans un menu.
- **Menu ☰** : Fichier, Édition, Exécution, Outils, Volets, Affichage, Aide, avec icônes et
  raccourcis (Ctrl+O ouvrir un fichier, Ctrl+Maj+O un dossier, Ctrl+G aller à la ligne…).
- **Panneau de gauche** : onglets Fichiers / SSH / VPN ; arbre aux mesures de Qt ; panneau SSH
  (liste déroulante modifiable, prise, « … », chemin, état en bas) et panneau VPN (Ajouter,
  liste, Garder internet, Connecter, clé, corbeille) identiques.
- **Éditeur** : marges de QScintilla (numéros, points d'arrêt, flèches de pliage) avec
  **pliage des blocs**, ligne du curseur bleutée, guides d'indentation en pointillés, accolade
  correspondante en orange, coloration identique (Python : ponctuation en cyan, `print`/`len`
  en blanc ; Gradle : vocabulaire en jaune), police 11 pt.
- **Panneau du bas** : Sortie, Interpréteur Python (nouveau : un vrai interpréteur), Terminal,
  Terminal SSH ; messages « ── Exécution : … ── » et champ de saisie comme la version Python.
- **Débogage** : panneau VARIABLES (Nom / Type / Valeur), champ « Évaluer », PILE D'APPELS.
- **Barre d'état** : bloc EDITION, état (« ⏸ débogage : à l'arrêt »…), fichier, position, Python.
- **Page d'accueil** : onglet « Accueil » avec les mêmes liens et favoris ; le tout premier
  lancement ouvre le fichier d'exemple `sans_titre_1.py`.
- **Recherche** : barre sous l'éditeur (↑, ↓, Remplacer, Tout) ; **PDF** : barre flottante qui
  se cache toute seule ; **Bibliothèques** : la même fenêtre que la version Python.
- Thèmes repris tels quels de la version Python.

## 2.0.0-alpha.3

- **Plusieurs terminaux** : clic droit sur un onglet du bas → « Nouveau terminal » (même machine
  SSH ou terminal local), bouton « + » à droite des onglets, Ctrl+Maj+`, « Nouveau terminal ici »
  sur un dossier distant, « Nouveau terminal » sur une machine du panneau SSH. Les terminaux en
  plus se ferment avec leur croix (ou clic milieu).
- **Apparence de la version Python** (façon LazyVim) : onglets avec icônes et trait de couleur en
  haut, pastilles de type de fichier identiques (`jv`, `gr`, `py`…), fichiers distants affichés
  « Serveur ⇄ App.java », barre d'état façon lualine (bloc ÉDITION, chemin du fichier, position,
  version de Python), barre d'outils avec écran partagé, Web, SSH et VPN, police du système pour
  l'interface, guides d'indentation et parenthèse correspondante encadrée.
- **Panneau SSH** comme la version Python : un sous-onglet par machine, sélecteur de machine avec
  la prise pour se déconnecter et un menu « … », chemin modifiable avec dossier parent et
  actualiser, état de la connexion en bas.
- Au premier lancement, le thème choisi dans la version Python est repris ; thème « Papier
  (clair) » ajouté.
- `studentcode .` dans le dossier de la séance précédente rouvre ses fichiers.

## 2.0.0-alpha.2

Toutes les fonctions de la version Python sont maintenant dans la version Rust (sauf l'aide par IA,
retirée pour l'instant).

- **Écran partagé** : glisser un onglet sur le bord droit ou bas d'un volet pour séparer
  (ou clic droit sur l'onglet, ou Ctrl+\), séparateur déplaçable, volets retenus.
- **SSH et labs** : connexion rapide (`ubuntu@10.11.0.2` ou `Serveur = ubuntu@ip`), machines
  enregistrées, mot de passe retenu, plusieurs machines en même temps, explorateur distant
  (ouvrir, créer, renommer, supprimer, déplacer en glissant, envoyer et télécharger), édition et
  enregistrement des fichiers distants, exécution sur la machine (F5) avec saisie, terminal SSH
  par machine, arrêt des programmes restés d'une séance précédente, diagnostic quand la machine
  ne répond pas (VPN, SSH non démarré…).
- **VPN** : NetworkManager sous Linux, OpenVPN GUI sous Windows ; import du fichier .ovpn,
  identifiants retenus, « garder internet » (seul le réseau du lab passe par le VPN), VPN
  connecté automatiquement avant la machine SSH qui en dépend.
- **Lecteur PDF** (module facultatif) : pages rendues à la demande, zoom, largeur / page entière,
  mode sombre, recherche surlignée, liens, page retenue pour chaque fichier.
- **Images** dans un onglet.
- **Bibliothèques Python** (module PyScripter) : liste, installation, suppression, suggestions.
- **Web** : favoris (cours URCA par défaut) ouverts dans le navigateur.
- Session enregistrée dès qu'elle change ; dans le terminal, les raccourcis Ctrl+lettre vont au
  shell ; Échap ferme les boîtes de dialogue ; enregistrement distant confirmé par la machine.

## 2.0.0-alpha.1

Première version de la réécriture en Rust, plus légère et plus rapide que la version Python.

- **Interface native** (egui, dessinée par la carte graphique) : environ 37 Mo pour l'application
  elle-même, aucun processeur utilisé au repos, démarrage quasi instantané.
- **Éditeur** : onglets, numéros de ligne, coloration de C, C++, Java, Python, Rust, Go,
  JavaScript, TypeScript, C#, Kotlin, shell, Makefile, HTML, CSS, JSON, Markdown, SQL…,
  avec les types en jaune, les fonctions en bleu et les constantes en orange.
  Indentation automatique, Tab / Maj+Tab sur plusieurs lignes, Ctrl+/ pour commenter,
  Ctrl+D pour dupliquer, Ctrl+F rechercher et remplacer.
- **Explorateur** : dossiers dépliés retenus, nouveau fichier / dossier, renommer,
  supprimer, glisser-déposer pour déplacer, fichiers créés ailleurs affichés tout seuls.
- **Terminal** intégré (bash, zsh… ou PowerShell sous Windows), qui suit le dossier ouvert.
- **Exécution** (F5) de C, C++ (raylib reconnu), Java (tous les .java du dossier, paquets),
  Python, JavaScript, Rust, Go, scripts ; saisie clavier envoyée au programme.
- **Module PyScripter** (facultatif, proposé à l'installation) : débogueur Python pas à pas
  (F6, F10, F11, Maj+F11), points d'arrêt (F9 ou clic dans la marge), pile d'appels,
  variables dépliables, évaluation d'expressions au point d'arrêt, console Python ouverte
  sur les variables après l'exécution.
- `studentcode .` ouvre le dossier courant ; la session (dossier, onglets, thème, points
  d'arrêt) est restaurée au lancement suivant ; 10 thèmes.

Pas encore dans cette version (restent dans la version Python, `studentcode-py`) : SSH et
labs, VPN, lecteur PDF, pages web, gestionnaire de bibliothèques Python, volets séparés.
