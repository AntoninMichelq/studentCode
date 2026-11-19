# StudentCode en Rust — journal des versions

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
