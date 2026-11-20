# Journal des versions

## 1.1.5

- SSH : plusieurs terminaux sur la même machine. Clic droit sur l'onglet « SSH … » du panneau du
  bas (ou sur la machine dans le panneau SSH, ou menu « … ») → **Nouveau terminal** ; sur un
  dossier de l'explorateur distant → **Nouveau terminal ici**. Les terminaux en plus ont une
  croix pour les fermer et se ferment à la déconnexion.

## 1.1.4

- Coloration plus riche en C, C++, Java, C#, JavaScript… : types (`Model`, `size_t`, `String`)
  en jaune, fonctions en bleu, constantes en MAJUSCULES (`SIZEMAP`, `NULL`) en orange,
  `#include` / `#define` en rose et fichiers inclus en vert.

## 1.1.3

- Ouvrir un dossier (menu, page d'accueil, `studentcode dossier`) place aussi le terminal dans ce
  dossier ; la saisie en cours dans le terminal est effacée avant.

## 1.1.2

- `studentcode .` dans un terminal ouvre StudentCode sur le dossier courant (explorateur et
  terminal dedans). Sous Windows, l'installateur ajoute la commande au PATH.
- Barre d'outils allégée : les boutons pas à pas n'apparaissent que pendant le débogage, Arrêter
  pendant une exécution, et Déboguer / point d'arrêt / bibliothèques pour un fichier Python.
- Le terminal est l'onglet affiché par défaut dans le panneau du bas.
- Windows : la croix de fermeture des onglets reprend l'icône de StudentCode (au lieu de la croix
  rouge de Qt).

## 1.1.1

- Fabrication des versions : l'autotest Windows indique précisément ce qui échoue, la version
  Linux est publiée même si celle de Windows échoue, et `publier.sh` laisse GitHub publier
  tous les fichiers (installateur Windows, zip, version Linux, sources).

## 1.1.0

- **Windows** : installateur (`…-windows-x64-setup.exe`, sans droits administrateur) et version
  portable (`.zip`). Terminal PowerShell intégré, C/C++ avec MinGW, Java, VPN via OpenVPN GUI.
- **Linux** : version prête à l'emploi (`…-linux-x86_64.tar.gz`), sans Python à installer.
- Les versions sont fabriquées et vérifiées automatiquement pour Windows et Linux.
- Mémoire : les pages web cachées s'endorment au bout d'une minute, Chromium limité à deux
  processus de rendu, modules lourds (SSH, PDF, web, console Python) chargés seulement au besoin.
  Au lancement : environ 115 Mo au lieu de 140 ; avec deux sites et l'IA ouverts : 230 à 295 Mo
  au lieu de 385.
- Option « Accélération graphique du web » (désactivée par défaut, économise de la mémoire).

## 1.0.0

Première version publiée.

- IDE Python façon PyScripter : éditeur, exécution, interpréteur conservé après l'exécution,
  débogueur pas à pas, gestionnaire de bibliothèques, complétion entre fichiers.
- Volets scindables avec onglets code, web, PDF et image ; session restaurée au lancement.
- Terminal intégré (local et SSH), explorateur de fichiers local et distant.
- Connexion SSH simplifiée (`utilisateur@machine`), diagnostic en cas d'échec.
- VPN OpenVPN via NetworkManager, limité au réseau du lab pour garder internet.
- Aide par IA dans un panneau (clé d'API ou modèle local), avec le fichier comme contexte.
- Coloration pour une soixantaine de types de fichiers (dont Gradle, Kotlin, YAML, HTML, SQL, Markdown,
  Dockerfile, Makefile) ; F5 compile et lance les fichiers C, C++ et Java.
- Plusieurs connexions SSH simultanées et nommées ; glisser-déposer dans l'explorateur distant.
- Nettoyage automatique à la connexion SSH (anciennes sessions, serveurs restés en écoute, java/python en arrière-plan).
- Page d'accueil : titre aux couleurs du logo.
- Lecteur PDF intégré, compact (barre flottante), avec recherche, sélection, liens, mode sombre et page retenue.
- Barres de défilement masquées par défaut, activables dans Affichage.
- Onze thèmes, mode Vim de base, icône de l'application.
