#!/usr/bin/env python3
"""StudentCode — un IDE Python façon PyScripter, avec un look LazyVim.

Éditeur à onglets, explorateur de fichiers, exécution, débogueur pas à pas
(points d'arrêt, variables, pile d'appels), interpréteur interactif et
gestionnaire de bibliothèques (pip).
"""
import json
import os
import re
import shlex
import shutil
import sys
import threading

__version__ = "1.1.5"
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from sysinfo import (APP_DIR, FROZEN, PYTHON_MISSING, WINDOWS, child_env, prepare_children, python_cmd,
                     python_exe, quote, restart_cmd, shell_cmd)
BACKEND = os.path.join(APP_DIR, "dbg_backend.py")
RUNNER = os.path.join(APP_DIR, "run_backend.py")
# chemin pour les feuilles de style : des « / » même sous Windows (un « \ » y casse le chemin,
# et Qt affiche alors sa croix rouge par défaut sur les onglets)
QSS_DIR = APP_DIR.replace("\\", "/")
END_RE = re.compile("\x1e__LAZYPY_END__(-?\\d+)\x1e")
NOISE = re.compile(r"^(\(\S+:\d+\): (Gtk|Gdk|GLib|GLib-GObject|Pango)-(WARNING|CRITICAL|Message)"
                   r"|qt\.qpa\.|qt\.glx|QRhi|Qt: Session management error|libpng warning: iCCP)")
QUIET_QT = "qt.qpa.wayland*=false;qt.qpa.services=false;qt.qpa.theme*=false"
STDIN_HINT = "Entrée du programme (stdin) — Entrée pour envoyer"
REPL_HINT = ">>> interpréteur actif : teste ton code ici, ex. Message([2, 3])"
sys.path.insert(0, APP_DIR)

from PyQt6.QtCore import QSize, QProcess, QProcessEnvironment, QSettings, Qt, QTimer, pyqtSignal, QFile, QEvent
from PyQt6.QtGui import (QAction, QColor, QFileSystemModel, QFont, QFontDatabase, QIcon,
                         QKeySequence, QTextCharFormat, QTextCursor)
from PyQt6.QtNetwork import QHostAddress, QTcpServer
from PyQt6.QtWidgets import (QApplication, QComboBox, QDialog, QFileDialog, QHBoxLayout,
                             QInputDialog, QLabel, QLineEdit, QListWidget, QMainWindow,
                             QMenu, QMessageBox, QPlainTextEdit, QPushButton, QSplitter, QSizePolicy, QToolButton, QAbstractItemView,
                             QTabWidget, QToolBar, QTreeView, QTreeWidget, QTreeWidgetItem,
                             QVBoxLayout, QWidget)
from PyQt6.Qsci import QsciAPIs, QsciLexerPython, QsciScintilla

import langs

from lazy import available, discard, wake

# Moteur web (Chromium, ~55 Mo rien qu'à charger) : importé seulement à la première page web.
# Il faut alors avoir partagé les contextes OpenGL avant la création de l'application.
HAS_WEB = available("PyQt6.QtWebEngineWidgets")
QWebEnginePage = QWebEngineProfile = QWebEngineSettings = QWebEngineView = WebView = None
from PyQt6.QtCore import QCoreApplication as _QCA, Qt as _Qt
_QCA.setAttribute(_Qt.ApplicationAttribute.AA_ShareOpenGLContexts)


def load_web():
    """Charge le moteur web au premier besoin."""
    global QWebEnginePage, QWebEngineProfile, QWebEngineSettings, QWebEngineView, WebView
    if WebView is None:
        from PyQt6.QtWebEngineCore import QWebEnginePage, QWebEngineProfile, QWebEngineSettings
        from PyQt6.QtWebEngineWidgets import QWebEngineView

        class WebView(QWebEngineView):
            def __init__(self, tab):
                super().__init__()
                self.tab = tab

            def createWindow(self, _type):  # liens « nouvel onglet » : ouverts dans le même volet
                new = self.tab.win.open_url(None, pane=self.tab.win.pane_of(self.tab))
                return new.view if new else None
        globals()["WebView"] = WebView


def chromium_flags():
    """Réglages du moteur web pour économiser la mémoire."""
    s = QSettings("StudentCode", "StudentCode")
    flags = ["--renderer-process-limit=2"]           # pages regroupées dans 2 processus au plus
    if not s.value("web_gpu", False, type=bool):
        flags.append("--disable-gpu")                # pas de processus graphique séparé (~70 Mo)
    return " ".join(flags)
from PyQt6.QtCore import QStandardPaths, QUrl
from PyQt6.QtGui import QDesktopServices, QDrag, QMouseEvent, QPainter, QPen, QPixmap
from PyQt6.QtCore import QMimeData
from PyQt6.QtWidgets import QTabBar
from PyQt6.QtWidgets import QScrollArea
from urllib.parse import quote_plus

from icons import IconProvider, file_icon, icon, pixmap
from remote import RemoteHub
from runcmd import posix_command, windows_command
from semantic import Semantic
from vpn import VpnPanel
from terminal import LocalTerminal, make_term
from ai import AiPanel
from pdfview import HAS_PDF, PdfTab


# ------------------------------------------------------------------ thème
# Palette Tokyo Night (le thème par défaut de LazyVim)
import theme
from theme import (BG, BG_DARK, BG_HL, BLUE, COMMENT, CURLINE_BG, CYAN, ERR_BG, FG, GREEN, GUTTER,
                   MAGENTA, ORANGE, RED, SEL, TEAL, YELLOW)

QSS = f"""
* {{ font-size: 10pt; }}
QMainWindow, QDialog, QWidget {{ background: {BG_DARK}; color: {FG}; }}
QMenuBar {{ background: {BG_DARK}; color: {FG}; }}
QMenuBar::item:selected, QMenu::item:selected {{ background: {BG_HL}; }}
QMenu {{ background: {BG}; border: 1px solid {GUTTER}; padding: 4px; }}
QMenu::item {{ padding: 5px 24px 5px 14px; border-radius: 4px; }}
QMenu::separator {{ height: 1px; background: {GUTTER}; margin: 4px 6px; }}
QToolBar {{ background: {BG_DARK}; border: none; border-bottom: 1px solid {BG}; spacing: 0px; padding: 3px; }}
QToolButton {{ padding: 5px 7px; border-radius: 6px; color: {FG}; font-weight: 600; }}
QToolButton:hover {{ background: {BG_HL}; }}
QToolButton:disabled {{ color: {GUTTER}; }}
QToolButton#run {{ color: {GREEN}; }} QToolButton#debug {{ color: {ORANGE}; }}
QToolButton#stop {{ color: {RED}; }} QToolButton#step {{ color: {BLUE}; }}
QToolButton#libs {{ color: {MAGENTA}; }}
QToolButton#run:disabled, QToolButton#debug:disabled, QToolButton#stop:disabled,
QToolButton#step:disabled {{ color: {GUTTER}; }}
QToolButton:checked {{ background: {BG_HL}; color: {GREEN}; }}
QTabWidget::pane {{ border: none; }}
QTabBar {{ background: {BG_DARK}; }}
QTabBar::tab {{ background: {BG_DARK}; color: {COMMENT}; padding: 6px 12px; border: none;
                border-top: 2px solid transparent; }}
QTabBar::tab:selected {{ background: {BG}; color: {FG}; border-top: 2px solid {BLUE}; }}
QTabBar::tab:hover {{ color: {FG}; }}
QTreeView, QTreeWidget, QListWidget {{ background: {BG_DARK}; border: none; outline: 0; }}
QTreeView::item, QTreeWidget::item, QListWidget::item {{ padding: 3px 2px; }}
QTreeView::item:selected, QTreeWidget::item:selected, QListWidget::item:selected
    {{ background: {BG_HL}; color: {FG}; }}
QHeaderView::section {{ background: {BG_DARK}; color: {COMMENT}; border: none; padding: 4px; }}
QPlainTextEdit {{ background: {BG}; border: none; selection-background-color: {SEL}; }}
QLineEdit, QComboBox {{ background: {BG}; border: 1px solid {GUTTER}; border-radius: 5px;
                        padding: 5px 7px; selection-background-color: {SEL}; }}
QLineEdit:focus, QComboBox:focus {{ border: 1px solid {BLUE}; }}
QComboBox QAbstractItemView {{ background: {BG}; selection-background-color: {BG_HL}; }}
QPushButton {{ background: {BG_HL}; border: none; border-radius: 5px; padding: 6px 12px; }}
QPushButton:hover {{ background: {GUTTER}; }}
QPushButton:disabled {{ color: {COMMENT}; }}
QPushButton#primary {{ background: {BLUE}; color: {BG_DARK}; font-weight: 600; }}
QSplitter::handle {{ background: {BG_DARK}; }}
QSplitter::handle:horizontal {{ width: 3px; }} QSplitter::handle:vertical {{ height: 3px; }}
QScrollBar:vertical {{ background: transparent; width: 8px; border: none; margin: 0; }}
QScrollBar:horizontal {{ background: transparent; height: 8px; border: none; margin: 0; }}
QScrollBar::handle {{ background: {GUTTER}; border-radius: 3px; min-height: 28px; min-width: 28px; margin: 1px; }}
QScrollBar::handle:hover {{ background: {COMMENT}; }}
QScrollBar::add-line, QScrollBar::sub-line {{ width: 0; height: 0; border: none; background: none; }}
QScrollBar::up-arrow, QScrollBar::down-arrow, QScrollBar::left-arrow, QScrollBar::right-arrow
    {{ width: 0; height: 0; background: none; border: none; }}
QAbstractScrollArea::corner {{ background: transparent; border: none; }}
QScrollBar::add-page, QScrollBar::sub-page {{ background: transparent; }}
QStatusBar {{ background: {BG_DARK}; color: {COMMENT}; min-height: 22px; }}
QStatusBar::item {{ border: none; }}
QTabBar::close-button {{ image: url("{QSS_DIR}/close.svg"); subcontrol-position: right; margin: 2px; }}
QTabBar::close-button:hover {{ image: url("{QSS_DIR}/close_hover.svg"); }}
QToolBar::separator {{ width: 1px; background: {BG_HL}; margin: 5px 6px; }}
QTabWidget[active="false"] QTabBar::tab:selected {{ border-top: 2px solid {GUTTER}; color: {COMMENT}; }}
QLabel#big {{ font-size: 24pt; font-weight: 800; }}
QLabel#sub {{ color: {COMMENT}; }}
QPushButton#link {{ text-align: left; padding: 8px 12px; background: {BG}; }}
QPushButton#link:hover {{ background: {BG_HL}; }}
QLabel#title {{ color: {COMMENT}; font-weight: 700; padding: 6px 8px; }}
QToolTip {{ background: {BG}; color: {FG}; border: 1px solid {GUTTER}; }}
"""

BP, CUR, CURLINE, ERRLINE = 1, 2, 3, 4  # marqueurs Scintilla

DEMO = '''# Bienvenue dans StudentCode !
#   F5  : exécuter          F6  : déboguer / continuer
#   F9  : point d'arrêt     F10 : pas à pas (par-dessus)
#   F11 : pas à pas (entrer dans la fonction)
# Clique dans la marge à gauche d'une ligne pour poser un point d'arrêt.

def fibonacci(n):
    suite = [0, 1]
    while len(suite) < n:
        suite.append(suite[-1] + suite[-2])
    return suite


def moyenne(valeurs):
    total = 0
    for v in valeurs:
        total += v
    return total / len(valeurs)


nombres = fibonacci(10)
print("Fibonacci :", nombres)
print("Moyenne   :", moyenne(nombres))
'''

SHORTCUTS = """<b>Exécution</b><br>
F5 — Exécuter &nbsp;·&nbsp; Maj+F5 — Arrêter<br>
F6 — Déboguer / Continuer<br>
F9 — Point d'arrêt (ou clic dans la marge)<br>
F10 — Pas à pas par-dessus &nbsp;·&nbsp; F11 — Entrer &nbsp;·&nbsp; Maj+F11 — Sortir<br><br>
<b>Fichiers</b><br>
Ctrl+N nouveau · Ctrl+O ouvrir · Ctrl+Maj+O ouvrir un dossier · Ctrl+S enregistrer · Ctrl+W fermer<br>
Ctrl+P — Recherche floue de fichiers (façon Telescope)<br><br>
<b>Édition</b><br>
Ctrl+F rechercher/remplacer · Ctrl+/ commenter · Ctrl+Espace complétion · Ctrl+G aller à la ligne<br>
Ctrl+B explorateur · Ctrl+J panneau du bas · Ctrl+molette zoom · Alt+M menu<br><br>
<b>Explorateur</b><br>
Ctrl+clic / Maj+clic sélection multiple · glisser-déposer pour déplacer · F2 renommer · Suppr corbeille<br>
La barre du haut sert à déplacer la fenêtre (double-clic : agrandir)<br><br>
<b>Volets</b><br>
glisser un onglet vers un bord du volet pour scinder de ce côté, ou vers un autre volet pour l'y déplacer<br>
Alt+V scinder à droite · Alt+H scinder en bas · Alt+W volet suivant · Alt+Maj+W déplacer l'onglet · Alt+Q fermer le volet<br>
Ctrl+T onglet web · clic droit sur un onglet ou un fichier pour l'ouvrir à côté · les PDF et images s'ouvrent dans un onglet<br><br>
<b>VPN</b><br>
onglet VPN à gauche : Ajouter (fichier .ovpn), puis Connecter · un lab SSH peut activer son VPN tout seul<br><br>
<b>SSH</b><br>
Ctrl+Alt+S se connecter à un lab · onglet SSH à gauche : fichiers de la machine · F5 exécute le fichier sur la machine · onglet Terminal SSH en bas (vrai terminal : vim, nano, Tab)<br><br>
<b>Complétion</b><br>
automatique pendant la frappe (y compris les classes de tes autres fichiers) · Ctrl+Espace pour la forcer · F12 aller à la définition<br><br>
<b>IA</b><br>
bouton « IA » en haut à droite (Alt+I) : panneau d'aide ; la clé jaune ouvre « Connexions IA » (clé d'API, modèle local ou site web) ; le fichier ouvert ou la sélection sert de contexte<br><br>
<b>Terminal</b><br>
Alt+T ouvre le terminal (ton shell, avec ton invite) · Ctrl+Maj+C / Ctrl+Maj+V copier-coller · Maj+PagePréc. remonter<br><br>
<b>Thèmes</b><br>
menu ☰ → Affichage → Thème (l'application se relance et rouvre ta session)<br><br>
<b>Mode Vim</b> (bouton « Vim » ou Ctrl+Alt+V)<br>
h j k l w b e 0 ^ $ gg G · i a I A o O · x dd yy p P D C cw cc ciw diw yiw · u Ctrl+R<br>
v / V (sélection) · / n N · &gt;&gt; &lt;&lt; · :w :q :wq :e fichier :42<br>
Maj+H / Maj+L onglet précédent/suivant · Espace e explorateur · Espace Espace fichiers · Espace b d fermer · Espace | et Espace - scinder · Espace w d fermer le volet · Ctrl+H / Ctrl+L changer de volet
"""

MOTIONS = {"h": "CHARLEFT", "l": "CHARRIGHT", "j": "LINEDOWN", "k": "LINEUP",
           "w": "WORDRIGHT", "b": "WORDLEFT", "e": "WORDRIGHTEND",
           "0": "HOME", "^": "VCHOME", "$": "LINEEND", "G": "DOCUMENTEND"}
VIM_PREFIXES = {"g", "d", "y", "c", " ", " f", " b", " w", "di", "ci", "yi", ">", "<"}


# ================================================================ éditeur
JEDI_LOCK = threading.Lock()  # jedi n'aime pas les appels simultanés
IDENT = re.compile(r"[A-Za-z_0-9À-ÿ]*$")


_jedi_env = []


def jedi_environment():
    """Python dont jedi lit les bibliothèques. Dans l'app empaquetée, celui de l'ordinateur
    (sinon jedi essaierait de lancer StudentCode lui-même comme interpréteur)."""
    if not _jedi_env:
        import jedi
        env = None
        if FROZEN:
            exe = python_exe()
            try:
                env = jedi.create_environment(exe, safe=False) if exe else jedi.InterpreterEnvironment()
            except Exception:
                env = jedi.InterpreterEnvironment()
        _jedi_env.append(env)
    return _jedi_env[0]


class Editor(QsciScintilla):
    comp_ready = pyqtSignal(int, int, int, list, bool)
    sig_ready = pyqtSignal(int, int, str)
    def __init__(self, win, path=None):
        super().__init__()
        self.win = win
        self.path = path
        self.mode = "NORMAL" if win.vim_on else "INSERT"
        self.pending = ""
        self.count = ""
        self.vl_anchor = 0
        self.remote = self.remote_lab = None  # fichier ouvert depuis un lab SSH
        self.remote_name = ""                  # nom donné à sa machine (Serveur, Croupier…)
        self.unsent = False  # enregistré en local mais pas encore envoyé sur la machine
        self.comp_id = 0
        self.comp_timer = QTimer(self)
        self.comp_timer.setSingleShot(True)
        self.comp_timer.setInterval(140)
        self.comp_timer.timeout.connect(self.auto_complete)
        self.comp_ready.connect(self.show_completions)
        self.sig_ready.connect(self.show_signature)
        self.setUtf8(True)
        self.setEolMode(QsciScintilla.EolMode.EolUnix)
        font = win.code_font
        self.setFont(font)
        self.setMarginsFont(font)
        self.setPaper(QColor(BG))
        self.setColor(QColor(FG))
        # marges : numéros, points d'arrêt, pliage
        self.setMarginLineNumbers(0, True)
        self.setMarginWidth(0, "00000")
        self.setMarginType(1, QsciScintilla.MarginType.SymbolMargin)
        self.setMarginWidth(1, 20)
        self.setMarginSensitivity(0, True)
        self.setMarginSensitivity(1, True)
        self.setMarginMarkerMask(0, 0)
        self.setMarginMarkerMask(1, (1 << BP) | (1 << CUR))
        self.setMarginsBackgroundColor(QColor(BG))
        self.setMarginsForegroundColor(QColor(GUTTER))
        self.setFolding(QsciScintilla.FoldStyle.PlainFoldStyle, 2)
        self.setFoldMarginColors(QColor(BG), QColor(BG))
        self.marginClicked.connect(lambda m, line, mods: self.toggle_bp(line) if m < 2 else None)
        # marqueurs
        self.markerDefine(QsciScintilla.MarkerSymbol.Circle, BP)
        self.setMarkerBackgroundColor(QColor(RED), BP)
        self.setMarkerForegroundColor(QColor(RED), BP)
        self.markerDefine(QsciScintilla.MarkerSymbol.RightArrow, CUR)
        self.setMarkerBackgroundColor(QColor(YELLOW), CUR)
        self.setMarkerForegroundColor(QColor(YELLOW), CUR)
        self.markerDefine(QsciScintilla.MarkerSymbol.Background, CURLINE)
        self.setMarkerBackgroundColor(QColor(CURLINE_BG), CURLINE)
        self.markerDefine(QsciScintilla.MarkerSymbol.Background, ERRLINE)
        self.setMarkerBackgroundColor(QColor(ERR_BG), ERRLINE)
        # curseur, sélection, indentation
        self.setCaretForegroundColor(QColor(FG))
        self.setCaretWidth(2)
        self.setCaretLineVisible(True)
        self.setCaretLineBackgroundColor(QColor(BLUE))
        self.SendScintilla(self.SCI_SETCARETLINEBACKALPHA, 45 if theme.LIGHT else 28)
        self.setSelectionBackgroundColor(QColor(SEL))
        self.resetSelectionForegroundColor()
        self.setBraceMatching(QsciScintilla.BraceMatch.SloppyBraceMatch)
        self.setMatchedBraceBackgroundColor(QColor(GUTTER))
        self.setMatchedBraceForegroundColor(QColor(ORANGE))
        self.setUnmatchedBraceBackgroundColor(QColor(BG))
        self.setUnmatchedBraceForegroundColor(QColor(RED))
        self.setIndentationsUseTabs(False)
        self.setTabWidth(4)
        self.setIndentationWidth(4)
        self.setAutoIndent(True)
        self.setBackspaceUnindents(True)
        self.setTabIndents(True)
        self.setIndentationGuides(True)
        self.setIndentationGuidesForegroundColor(QColor(BG_HL))
        self.setIndentationGuidesBackgroundColor(QColor(BG))
        self.SendScintilla(self.SCI_SETSCROLLWIDTH, 1)
        self.SendScintilla(self.SCI_SETSCROLLWIDTHTRACKING, 1)
        win.style_scrollbars(self)
        # complétion (mots du document + jedi)
        self.setAutoCompletionSource(QsciScintilla.AutoCompletionSource.AcsNone)
        self.SendScintilla(self.SCI_CALLTIPSETBACK, QColor(BG_HL))
        self.SendScintilla(self.SCI_CALLTIPSETFORE, QColor(FG))
        self.setAutoCompletionCaseSensitivity(False)
        self.SendScintilla(self.SCI_AUTOCSETORDER, 1)
        self.SendScintilla(self.SCI_AUTOCSETIGNORECASE, 1)
        self.setCallTipsVisible(0)
        self.apply_lexer()
        self.fold_arrows()
        self.set_mode(self.mode)
        self.cursorPositionChanged.connect(lambda *_: win.update_status())

    def fold_arrows(self):
        """Marge de pliage : un chevron vers le bas (bloc ouvert, clic pour replier)
        ou vers la droite (bloc replié, clic pour déplier), sans traits ni cases."""
        B = QsciScintilla
        self.setMarginWidth(2, 18)
        opened = pixmap("chev_down", COMMENT, 14, 1.0)
        closed = pixmap("chev_right", BLUE, 14, 1.0)
        for num, pm in ((B.SC_MARKNUM_FOLDEROPEN, opened), (B.SC_MARKNUM_FOLDEROPENMID, opened),
                        (B.SC_MARKNUM_FOLDER, closed), (B.SC_MARKNUM_FOLDEREND, closed)):
            self.markerDefine(pm, num)
        for num in (B.SC_MARKNUM_FOLDERSUB, B.SC_MARKNUM_FOLDERTAIL, B.SC_MARKNUM_FOLDERMIDTAIL):
            self.SendScintilla(B.SCI_MARKERDEFINE, num, B.SC_MARK_EMPTY)
        self.SendScintilla(B.SCI_SETFOLDFLAGS, 0)  # pas de trait sous un bloc replié

    # ------------------------------------------------------------- lexer
    def is_python(self):
        return self.path is None or self.path.endswith((".py", ".pyw", ".pyi"))

    def ext(self):
        return os.path.splitext(self.path or "x.py")[1].lower()

    def comment_prefix(self):
        marks = langs.comment_marks(self.path)
        return marks[0] if marks else ""

    def apply_lexer(self):
        key = langs.detect(self.path) if not self.is_python() else None
        if key:   # C, Java, Gradle, Kotlin, HTML, YAML, SQL… : coloration aux couleurs du thème
            lx = langs.theme_lexer(langs.make_lexer(key, self), self.win.code_font, theme.P)
            self.setLexer(lx)
            if Semantic.supported(lx):   # types, fonctions et constantes colorés en plus des mots-clés
                if getattr(self, "semantic", None) is not None:
                    self.textChanged.disconnect(self.semantic.timer.start)
                self.semantic = Semantic(self, key, {"prepro": theme.P["RED"], "type": theme.P["YELLOW"],
                                                     "func": theme.P["BLUE"], "const": theme.P["ORANGE"],
                                                     "include": theme.P["GREEN"]})
            self.setMarginsBackgroundColor(QColor(BG))
            self.setMarginsForegroundColor(QColor(GUTTER))
            self.setFoldMarginColors(QColor(BG), QColor(BG))
            self.setMatchedBraceBackgroundColor(QColor(GUTTER))
            self.setMatchedBraceForegroundColor(QColor(ORANGE))
            words = langs.keyword_list(lx)
            if words:   # complétion : mots du fichier + mots-clés du langage
                self.apis = QsciAPIs(lx)
                for w in words:
                    self.apis.add(w)
                self.apis.prepare()
            self.setAutoCompletionSource(QsciScintilla.AutoCompletionSource.AcsAll)
            self.setAutoCompletionThreshold(3)
            return
        if not self.is_python():
            self.setLexer(None)
            self.setPaper(QColor(BG))
            self.setColor(QColor(FG))
            self.setAutoCompletionSource(QsciScintilla.AutoCompletionSource.AcsDocument)
            self.setAutoCompletionThreshold(3)
            return
        self.setAutoCompletionSource(QsciScintilla.AutoCompletionSource.AcsNone)
        lx = QsciLexerPython(self)
        font = self.win.code_font
        lx.setDefaultFont(font)
        lx.setDefaultPaper(QColor(BG))
        lx.setDefaultColor(QColor(FG))
        lx.setFont(font)
        lx.setPaper(QColor(BG))
        lx.setColor(QColor(FG))
        P = QsciLexerPython
        colors = {"Comment": COMMENT, "CommentBlock": COMMENT, "Number": ORANGE,
                  "DoubleQuotedString": GREEN, "SingleQuotedString": GREEN,
                  "TripleSingleQuotedString": GREEN, "TripleDoubleQuotedString": GREEN,
                  "DoubleQuotedFString": GREEN, "SingleQuotedFString": GREEN,
                  "TripleSingleQuotedFString": GREEN, "TripleDoubleQuotedFString": GREEN,
                  "UnclosedString": RED, "Keyword": MAGENTA, "ClassName": YELLOW,
                  "FunctionMethodName": BLUE, "Operator": CYAN, "Identifier": FG,
                  "HighlightedIdentifier": TEAL, "Decorator": YELLOW}
        for name, col in colors.items():
            style = getattr(P, name, None)
            if style is not None:
                lx.setColor(QColor(col), style)
        italic = QFont(font)
        italic.setItalic(True)
        lx.setFont(italic, P.Comment)
        lx.setFont(italic, P.CommentBlock)
        self.setLexer(lx)
        self.setMarginsBackgroundColor(QColor(BG))
        self.setMarginsForegroundColor(QColor(GUTTER))
        self.setFoldMarginColors(QColor(BG), QColor(BG))
        self.setMatchedBraceBackgroundColor(QColor(GUTTER))
        self.setMatchedBraceForegroundColor(QColor(ORANGE))

    # ----------------------------------------------------- points d'arrêt
    def breakpoints(self):
        out, line = [], self.markerFindNext(0, 1 << BP)
        while line >= 0:
            out.append(line + 1)
            line = self.markerFindNext(line + 1, 1 << BP)
        return out

    def toggle_bp(self, line):
        on = not (self.markersAtLine(line) & (1 << BP))
        if on:
            if not self.text(line).strip():
                return
            self.markerAdd(line, BP)
        else:
            self.markerDelete(line, BP)
        self.win.bp_changed(self, line + 1, on)

    # ---------------------------------------------------------- complétion
    def jedi_script(self, text):
        """Analyse du fichier avec son dossier et le projet : les classes et fonctions
        des autres fichiers importés sont connues, comme les bibliothèques installées."""
        import jedi
        here = os.path.dirname(self.path) if self.path else self.win.root
        extra = [d for d in dict.fromkeys([here, self.win.root]) if d and os.path.isdir(d)]
        return jedi.Script(text, path=self.path, project=jedi.Project(here, added_sys_path=extra),
                           environment=jedi_environment())

    def complete(self, auto=False):
        if not self.is_python():
            if not auto:
                self.autoCompleteFromDocument()
            return
        self.comp_id += 1
        rid, text = self.comp_id, self.text()
        line, col = self.getCursorPosition()

        def work():
            names = []
            try:
                with JEDI_LOCK:
                    comps = self.jedi_script(text).complete(line + 1, col)
                    names = sorted({c.name for c in comps[:400] if c.name and " " not in c.name})
            except Exception:
                pass
            try:
                self.comp_ready.emit(rid, line, col, names, auto)
            except RuntimeError:  # onglet fermé entre-temps
                pass
        threading.Thread(target=work, daemon=True).start()

    def show_completions(self, rid, line, col, names, auto):
        if rid != self.comp_id or (line, col) != self.getCursorPosition() or not self.hasFocus():
            return
        if not names:
            if not auto:
                self.autoCompleteFromDocument()
            return
        prefix = IDENT.search(self.text(line)[:col]).group(0)
        if auto and names == [prefix]:
            return
        self.SendScintilla(self.SCI_AUTOCSETSEPARATOR, ord(" "))
        self.SendScintilla(self.SCI_AUTOCSHOW, len(prefix.encode("utf-8")), " ".join(names).encode("utf-8"))

    def auto_complete(self):
        """Proposée toute seule pendant la frappe (après un point ou deux lettres)."""
        if not self.is_python() or self.isListActive() or not self.hasFocus():
            return
        if self.win.vim_on and self.mode != "INSERT":
            return
        line, col = self.getCursorPosition()
        before = self.text(line)[:col]
        pos = self.SendScintilla(self.SCI_GETCURRENTPOS)
        style = self.SendScintilla(self.SCI_GETSTYLEAT, max(pos - 1, 0))
        P = QsciLexerPython
        if style in (P.Comment, P.CommentBlock, P.SingleQuotedString, P.DoubleQuotedString,
                     P.TripleSingleQuotedString, P.TripleDoubleQuotedString):
            return
        prefix = IDENT.search(before).group(0)
        if before.endswith(".") or (len(prefix) >= 2 and not prefix[0].isdigit()):
            self.complete(auto=True)

    def signature(self):
        """Info-bulle des paramètres après une parenthèse ouvrante."""
        self.comp_id += 1
        rid, text = self.comp_id, self.text()
        line, col = self.getCursorPosition()
        pos = self.SendScintilla(self.SCI_GETCURRENTPOS)

        def work():
            tip = ""
            try:
                with JEDI_LOCK:
                    sigs = self.jedi_script(text).get_signatures(line + 1, col)
                    tip = "\n".join(s.to_string() for s in sigs[:3])
            except Exception:
                pass
            try:
                self.sig_ready.emit(rid, pos, tip)
            except RuntimeError:
                pass
        threading.Thread(target=work, daemon=True).start()

    def show_signature(self, rid, pos, tip):
        if tip and rid == self.comp_id and pos == self.SendScintilla(self.SCI_GETCURRENTPOS):
            self.SendScintilla(self.SCI_CALLTIPSHOW, pos, tip.encode("utf-8"))

    def goto_definition(self):
        if not self.is_python():
            return
        line, col = self.getCursorPosition()
        try:
            with JEDI_LOCK:
                defs = self.jedi_script(self.text()).goto(line + 1, col, follow_imports=True)
        except Exception:
            defs = []
        for d in defs:
            if d.line and (d.module_path or not self.path):
                target = str(d.module_path) if d.module_path else None
                if target is None or target == self.path:
                    self.setCursorPosition(d.line - 1, d.column or 0)
                    self.ensureLineVisible(d.line - 1)
                elif os.path.isfile(target):
                    self.win.open_file(target, d.line)
                return
        self.win.statusBar().showMessage("Définition introuvable.", 3000)

    def toggle_comment(self):
        marks = langs.comment_marks(self.path)
        if not marks:
            self.win.statusBar().showMessage("Ce type de fichier n'a pas de commentaires.", 3000)
            return
        start, end = marks
        if self.hasSelectedText():
            l1, _, l2, i2 = self.getSelection()
            if i2 == 0 and l2 > l1:
                l2 -= 1
        else:
            l1 = l2 = self.getCursorPosition()[0]
        lines = [self.text(i).rstrip("\r\n") for i in range(l1, l2 + 1)]
        body = [s for s in lines if s.strip()]
        uncomment = bool(body) and all(s.lstrip().startswith(start) and s.rstrip().endswith(end)
                                       for s in body)
        self.beginUndoAction()
        for i, s in zip(range(l1, l2 + 1), lines):
            if not s.strip():
                continue
            ind = len(s) - len(s.lstrip())
            if uncomment:
                if end:   # retire la fin d'abord, la position du début ne bouge pas
                    stop = len(s.rstrip())
                    cut = len(end) + (1 if s.rstrip()[:-len(end)].endswith(" ") else 0)
                    self.setSelection(i, stop - cut, i, stop)
                    self.removeSelectedText()
                    s = self.text(i).rstrip("\r\n")
                n = len(start) + (1 if s[ind + len(start):ind + len(start) + 1] == " " else 0)
                self.setSelection(i, ind, i, ind + n)
                self.removeSelectedText()
            else:
                if end:
                    self.insertAt(" " + end, i, len(s))
                self.insertAt(start + " ", i, ind)
        self.endUndoAction()

    # ------------------------------------------------------------ clavier
    def keyPressEvent(self, e):
        ctrl = bool(e.modifiers() & Qt.KeyboardModifier.ControlModifier)
        if ctrl and e.key() == Qt.Key.Key_Space:
            self.complete()
            return
        if self.win.vim_on and self.vim_key(e):
            return
        super().keyPressEvent(e)
        t = e.text()
        if not self.is_python() or not t:
            return
        if t == "(":
            self.signature()
        elif t == ")":
            self.SendScintilla(self.SCI_CALLTIPCANCEL)
        elif t == "." or t.isalnum() or t == "_":
            self.comp_timer.start()

    # --------------------------------------------------------------- vim
    def sci(self, name, n=1):
        c = getattr(self, "SCI_" + name)
        for _ in range(max(1, n)):
            self.SendScintilla(c)

    def set_mode(self, mode):
        self.mode = mode
        self.pending = self.count = ""
        block = self.win.vim_on and mode != "INSERT"
        self.SendScintilla(self.SCI_SETCARETSTYLE, 2 if block else 1)
        self.win.update_status()

    def sel_lines(self, l1, l2):
        a = self.positionFromLineIndex(l1, 0)
        b = self.positionFromLineIndex(l2 + 1, 0) if l2 + 1 < self.lines() else self.length()
        self.SendScintilla(self.SCI_SETSEL, a, b)

    def collapse(self):
        self.SendScintilla(self.SCI_SETEMPTYSELECTION,
                           self.SendScintilla(self.SCI_GETSELECTIONSTART))

    def vline_update(self):
        cur = self.getCursorPosition()[0]
        a = self.vl_anchor
        eol = lambda l: len(self.text(l).rstrip("\r\n"))
        if cur >= a:
            self.setSelection(a, 0, cur, eol(cur))
        else:
            self.setSelection(a, eol(a), cur, 0)

    def vim_key(self, e):
        k, t, mod = e.key(), e.text(), e.modifiers()
        K = Qt.Key
        if self.mode == "INSERT":
            if k == K.Key_Escape:
                if self.isListActive():
                    self.cancelList()
                self.set_mode("NORMAL")
                return True
            return False
        if k == K.Key_Escape:
            if self.mode != "NORMAL":
                self.SendScintilla(self.SCI_SETEMPTYSELECTION,
                                   self.SendScintilla(self.SCI_GETCURRENTPOS))
            self.set_mode("NORMAL")
            return True
        if mod & Qt.KeyboardModifier.ControlModifier:
            if k == K.Key_R:
                self.redo()
                return True
            if k in (K.Key_H, K.Key_K):
                self.win.next_pane(-1)
                return True
            if k in (K.Key_L, K.Key_J):
                self.win.next_pane(1)
                return True
            return False
        if mod & Qt.KeyboardModifier.AltModifier:
            return False
        if not t or not t.isprintable():
            return k in (K.Key_Return, K.Key_Enter, K.Key_Backspace, K.Key_Tab, K.Key_Delete)
        if not self.pending and t.isdigit() and (t != "0" or self.count):
            self.count += t
            return True
        seq = self.pending + t
        res = self.vim_exec(seq, int(self.count or 1))
        if res is None:
            self.pending = seq
        else:
            self.pending = self.count = ""
        return True

    def vim_exec(self, s, n):
        w = self.win
        visual = self.mode in ("VISUAL", "V-LINE")
        ext = "EXTEND" if self.mode == "VISUAL" else ""
        clip = QApplication.clipboard()
        if s in MOTIONS or s == "gg":
            if s == "G" and self.count:
                self.setCursorPosition(n - 1, 0)
            elif s == "gg":
                self.sci("DOCUMENTSTART" + ext)
            else:
                self.sci(MOTIONS[s] + ext, 1 if s in "0^$G" else n)
            if self.mode == "V-LINE":
                self.vline_update()
            return True
        if visual:
            if s in ("y", "d", "x", "c"):
                linewise = self.mode == "V-LINE"
                if linewise:
                    l1, _, l2, _ = self.getSelection()
                    self.sel_lines(l1, l2)
                w.vim_linewise = linewise
                if s == "y":
                    self.copy()
                    self.collapse()
                else:
                    self.cut()
                self.set_mode("INSERT" if s == "c" else "NORMAL")
                return True
            if s in (">", "<"):
                l1, _, l2, _ = self.getSelection()
                for line in range(l1, l2 + 1):
                    self.indent(line) if s == ">" else self.unindent(line)
                self.collapse()
                self.set_mode("NORMAL")
                return True
            if s in ("v", "V"):
                self.collapse()
                self.set_mode("NORMAL")
                return True
            return None if s == "g" else False
        line, idx = self.getCursorPosition()
        pos = self.SendScintilla(self.SCI_GETCURRENTPOS)
        if s in VIM_PREFIXES:
            return None
        if s == "i":
            self.set_mode("INSERT")
        elif s == "a":
            if idx < len(self.text(line).rstrip("\r\n")):
                self.sci("CHARRIGHT")
            self.set_mode("INSERT")
        elif s == "I":
            self.sci("VCHOME")
            self.set_mode("INSERT")
        elif s == "A":
            self.sci("LINEEND")
            self.set_mode("INSERT")
        elif s == "o":
            self.sci("LINEEND")
            self.sci("NEWLINE")
            self.set_mode("INSERT")
        elif s == "O":
            indent = self.indentation(line)
            self.insertAt("\n", line, 0)
            self.setIndentation(line, indent)
            self.setCursorPosition(line, indent)
            self.set_mode("INSERT")
        elif s == "x":
            for _ in range(n):
                if self.getCursorPosition()[1] < len(self.text(line).rstrip("\r\n")):
                    self.sci("CLEAR")
        elif s in ("dd", "yy"):
            self.sel_lines(line, min(line + n - 1, self.lines() - 1))
            w.vim_linewise = True
            if s == "dd":
                self.cut()
                self.sci("VCHOME")
            else:
                self.copy()
                self.setCursorPosition(line, idx)
        elif s in ("D", "d$"):
            self.sci("DELLINERIGHT")
        elif s in ("C", "c$"):
            self.sci("DELLINERIGHT")
            self.set_mode("INSERT")
        elif s == "dw":
            self.sci("DELWORDRIGHT", n)
        elif s == "cw":
            self.sci("WORDRIGHTENDEXTEND", n)
            w.vim_linewise = False
            self.cut()
            self.set_mode("INSERT")
        elif s == "cc":
            self.sci("VCHOME")
            self.sci("LINEENDEXTEND")
            w.vim_linewise = False
            self.cut()
            self.set_mode("INSERT")
        elif s in ("diw", "ciw", "yiw"):
            a = self.SendScintilla(self.SCI_WORDSTARTPOSITION, pos, 1)
            b = self.SendScintilla(self.SCI_WORDENDPOSITION, pos, 1)
            self.SendScintilla(self.SCI_SETSEL, a, b)
            w.vim_linewise = False
            if s == "yiw":
                self.copy()
                self.collapse()
            else:
                self.cut()
                if s == "ciw":
                    self.set_mode("INSERT")
        elif s in ("p", "P"):
            text = clip.text()
            if not text:
                return True
            self.beginUndoAction()
            if w.vim_linewise:
                if not text.endswith("\n"):
                    text += "\n"
                if s == "P":
                    self.insertAt(text, line, 0)
                    self.setCursorPosition(line, 0)
                elif line + 1 < self.lines():
                    self.insertAt(text, line + 1, 0)
                    self.setCursorPosition(line + 1, 0)
                else:
                    self.sci("DOCUMENTEND")
                    self.insert("\n" + text.rstrip("\n"))
                    self.setCursorPosition(line + 1, 0)
                self.sci("VCHOME")
            else:
                if s == "p" and idx < len(self.text(line).rstrip("\r\n")):
                    self.sci("CHARRIGHT")
                self.paste()
            self.endUndoAction()
        elif s == "u":
            for _ in range(n):
                self.undo()
        elif s == "v":
            self.set_mode("VISUAL")
        elif s == "V":
            self.vl_anchor = line
            self.set_mode("V-LINE")
            self.vline_update()
        elif s in (">>", "<<"):
            for ln in range(line, min(line + n, self.lines())):
                self.indent(ln) if s == ">>" else self.unindent(ln)
        elif s == "/":
            w.show_find()
        elif s == "n":
            w.find_next(True)
        elif s == "N":
            w.find_next(False)
        elif s == ":":
            w.show_cmdline()
        elif s == "H":
            w.next_tab(-1)
        elif s == "L":
            w.next_tab(1)
        elif s == " e":
            w.toggle_explorer()
        elif s in ("  ", " ff"):
            w.show_finder()
        elif s == " bd":
            w.close_tab(w.tabs.currentIndex())
        elif s == " ft":
            w.show_terminal()
        elif s == " |":
            w.split(Qt.Orientation.Horizontal)
        elif s == " -":
            w.split(Qt.Orientation.Vertical)
        elif s == " wd":
            w.close_pane()
        elif s == " ww":
            w.next_pane(1)
        else:
            return False
        return True


# ================================================================ widgets
class Line(QLineEdit):
    """Champ de saisie avec historique (flèches haut/bas) et Échap."""
    submitted = pyqtSignal(str)
    escaped = pyqtSignal()

    def __init__(self, placeholder=""):
        super().__init__()
        self.setPlaceholderText(placeholder)
        self.hist, self.hi = [], 0
        self.keep = False

    def keyPressEvent(self, e):
        k = e.key()
        if k in (Qt.Key.Key_Return, Qt.Key.Key_Enter):
            t = self.text()
            if t.strip():
                self.hist.append(t)
            self.hi = len(self.hist)
            if not self.keep:
                self.clear()
            self.submitted.emit(t)
        elif k == Qt.Key.Key_Escape:
            self.escaped.emit()
        elif k == Qt.Key.Key_Up and self.hist and not self.keep:
            self.hi = max(0, self.hi - 1)
            self.setText(self.hist[self.hi])
        elif k == Qt.Key.Key_Down and self.hist and not self.keep:
            self.hi = min(len(self.hist), self.hi + 1)
            self.setText(self.hist[self.hi] if self.hi < len(self.hist) else "")
        else:
            super().keyPressEvent(e)


class Console(QWidget):
    """Zone de sortie colorée + ligne de saisie."""
    submitted = pyqtSignal(str)
    TRACE = re.compile(r'File "(.+?)", line (\d+)')

    def __init__(self, win, placeholder):
        super().__init__()
        self.win = win
        self.out = QPlainTextEdit()
        self.out.setReadOnly(True)
        self.out.setFont(win.code_font)
        self.out.setMaximumBlockCount(8000)
        self.out.viewport().installEventFilter(self)
        self.inp = Line(placeholder)
        self.inp.setFont(win.code_font)
        self.inp.submitted.connect(self.submitted)
        lay = QVBoxLayout(self)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.setSpacing(0)
        lay.addWidget(self.out)
        lay.addWidget(self.inp)

    def write(self, text, color=FG):
        cur = self.out.textCursor()
        cur.movePosition(QTextCursor.MoveOperation.End)
        fmt = QTextCharFormat()
        fmt.setForeground(QColor(color))
        cur.insertText(text, fmt)
        self.out.setTextCursor(cur)
        self.out.ensureCursorVisible()

    def eventFilter(self, obj, ev):
        # double-clic sur une ligne de traceback -> ouvre le fichier à la ligne
        if ev.type() == QEvent.Type.MouseButtonDblClick:
            block = self.out.cursorForPosition(ev.position().toPoint()).block().text()
            m = self.TRACE.search(block)
            if m and os.path.isfile(m.group(1)):
                self.win.open_file(m.group(1), int(m.group(2)))
                return True
        return False


class Finder(QDialog):
    """Recherche floue de fichiers (Ctrl+P), à la Telescope."""
    SKIP = {"__pycache__", "node_modules", "venv", ".venv", "env", "build", "dist"}

    def __init__(self, win):
        super().__init__(win, Qt.WindowType.FramelessWindowHint | Qt.WindowType.Dialog)
        self.win = win
        self.setStyleSheet(f"QDialog {{ border: 1px solid {BLUE}; background: {BG}; }}")
        self.resize(620, 380)
        self.q = Line("  Rechercher un fichier…")
        self.q.keep = True
        self.list = QListWidget()
        lay = QVBoxLayout(self)
        lay.addWidget(self.q)
        lay.addWidget(self.list)
        self.files = []
        root = win.root
        for d, dirs, files in os.walk(root):
            dirs[:] = sorted(x for x in dirs if not x.startswith(".") and x not in self.SKIP)
            for f in sorted(files):
                if not f.startswith(".") and not f.endswith((".pyc", ".o", ".so")):
                    self.files.append(os.path.relpath(os.path.join(d, f), root))
            if len(self.files) > 6000:
                break
        self.q.textChanged.connect(self.refresh)
        self.q.submitted.connect(lambda _: self.accept_item())
        self.q.escaped.connect(self.reject)
        self.list.itemActivated.connect(lambda _: self.accept_item())
        self.list.itemClicked.connect(lambda _: self.accept_item())
        self.q.installEventFilter(self)
        self.refresh("")

    @staticmethod
    def fuzzy(pat, s):
        it = iter(s)
        return all(c in it for c in pat)

    def refresh(self, text):
        pat = text.lower().replace(" ", "")
        hits = [f for f in self.files if self.fuzzy(pat, f.lower())]
        hits.sort(key=lambda f: (pat not in os.path.basename(f).lower(), len(f)))
        self.list.clear()
        self.list.addItems(hits[:200])
        self.list.setCurrentRow(0)

    def eventFilter(self, obj, ev):
        if ev.type() == QEvent.Type.KeyPress and ev.key() in (Qt.Key.Key_Up, Qt.Key.Key_Down):
            d = 1 if ev.key() == Qt.Key.Key_Down else -1
            n = self.list.count()
            if n:
                self.list.setCurrentRow((self.list.currentRow() + d) % n)
            return True
        return False

    def accept_item(self):
        it = self.list.currentItem()
        if it:
            self.win.open_file(os.path.join(self.win.root, it.text()))
        self.accept()


class Packages(QDialog):
    """Gestionnaire de bibliothèques : liste, installation, désinstallation via pip."""
    POPULAR = ["numpy", "pandas", "matplotlib", "scipy", "scikit-learn", "sympy", "requests",
               "beautifulsoup4", "pillow", "pygame", "seaborn", "networkx", "flask", "fastapi",
               "django", "pytest", "opencv-python", "torch", "tensorflow", "plotly", "tqdm",
               "openpyxl", "sqlalchemy", "pyserial", "pyyaml", "rich", "numba", "jupyter"]

    def __init__(self, win):
        super().__init__(win)
        self.setWindowTitle("Bibliothèques Python")
        self.resize(720, 560)
        self.proc = None
        self.pkgs = []
        self.name = QComboBox()
        self.name.setEditable(True)
        self.name.addItems(self.POPULAR)
        self.name.setCurrentText("")
        self.name.lineEdit().setPlaceholderText("Nom du paquet à installer (ex : requests, numpy==2.1)")
        self.name.lineEdit().returnPressed.connect(self.install)
        self.b_inst = QPushButton("Installer")
        self.b_inst.setObjectName("primary")
        self.b_inst.clicked.connect(self.install)
        top = QHBoxLayout()
        top.addWidget(self.name, 1)
        top.addWidget(self.b_inst)
        self.filter = QLineEdit()
        self.filter.setPlaceholderText("Filtrer les bibliothèques installées…")
        self.filter.textChanged.connect(self.show_list)
        self.tree = QTreeWidget()
        self.tree.setHeaderLabels(["Bibliothèque", "Version"])
        self.tree.setRootIsDecorated(False)
        self.tree.setColumnWidth(0, 420)
        self.b_up = QPushButton("Mettre à jour")
        self.b_rm = QPushButton("Désinstaller")
        self.b_ref = QPushButton("Rafraîchir")
        self.b_up.clicked.connect(lambda: self.selected("install", "--upgrade"))
        self.b_rm.clicked.connect(lambda: self.selected("uninstall", "-y"))
        self.b_ref.clicked.connect(self.refresh)
        row = QHBoxLayout()
        self.info = QLabel("")
        self.info.setStyleSheet(f"color: {COMMENT};")
        row.addWidget(self.info, 1)
        for b in (self.b_up, self.b_rm, self.b_ref):
            row.addWidget(b)
        self.log = QPlainTextEdit()
        self.log.setReadOnly(True)
        self.log.setFont(win.code_font)
        self.log.setMaximumHeight(170)
        lay = QVBoxLayout(self)
        lay.addLayout(top)
        lay.addWidget(self.filter)
        lay.addWidget(self.tree, 1)
        lay.addLayout(row)
        lay.addWidget(self.log)
        self.refresh()

    def busy(self, on):
        for b in (self.b_inst, self.b_up, self.b_rm, self.b_ref):
            b.setEnabled(not on)

    def pip(self, args, capture=None):
        if self.proc:
            return
        self.busy(True)
        p = self.proc = QProcess(self)
        buf = []
        if capture is None:
            self.log.appendPlainText("$ pip " + " ".join(args))
            p.setProcessChannelMode(QProcess.ProcessChannelMode.MergedChannels)
            p.readyReadStandardOutput.connect(lambda: self.log_add(
                bytes(p.readAllStandardOutput()).decode("utf-8", "replace")))
        else:
            p.readyReadStandardOutput.connect(lambda: buf.append(bytes(p.readAllStandardOutput())))

        def done(*_):
            self.proc = None
            self.busy(False)
            p.deleteLater()
            if capture:
                capture(b"".join(buf).decode("utf-8", "replace"))
            else:
                self.refresh()
        p.finished.connect(done)
        p.errorOccurred.connect(lambda _: done() if p.state() == QProcess.ProcessState.NotRunning
                                and self.proc is p else None)
        py = python_cmd()
        if py is None:
            self.proc = None
            self.busy(False)
            self.log.appendPlainText(PYTHON_MISSING)
            return
        p.start(py[0], py[1:] + ["-m", "pip", "--disable-pip-version-check"] + args)

    def log_add(self, text):
        self.log.moveCursor(QTextCursor.MoveOperation.End)
        self.log.insertPlainText(text)
        self.log.ensureCursorVisible()

    def refresh(self):
        self.info.setText("Chargement…")
        self.pip(["list", "--format=json"], capture=self.loaded)

    def loaded(self, text):
        try:
            self.pkgs = json.loads(text[text.index("["):])
        except ValueError:
            self.pkgs = []
        self.show_list()

    def show_list(self, *_):
        f = self.filter.text().lower()
        self.tree.clear()
        for p in self.pkgs:
            if f in p["name"].lower():
                self.tree.addTopLevelItem(QTreeWidgetItem([p["name"], p["version"]]))
        py = python_cmd()
        self.info.setText(f"{len(self.pkgs)} bibliothèques · " + (" ".join(py) if FROZEN and py else sys.prefix))

    def install(self):
        spec = self.name.currentText().strip()
        if spec:
            self.pip(["install"] + shlex.split(spec))

    def selected(self, *cmd):
        it = self.tree.currentItem()
        if it:
            self.pip(list(cmd) + [it.text(0)])


class FileTree(QTreeView):
    """Explorateur : sélection multiple (Ctrl/Maj) et glisser-déposer pour déplacer."""

    def __init__(self, win):
        super().__init__()
        self.win = win
        self.setSelectionMode(QAbstractItemView.SelectionMode.ExtendedSelection)
        self.setDragEnabled(True)
        self.setAcceptDrops(True)
        self.setDropIndicatorShown(True)
        self.setDragDropMode(QAbstractItemView.DragDropMode.DragDrop)
        self.setDefaultDropAction(Qt.DropAction.MoveAction)
        self.setAutoExpandDelay(600)

    def selected_paths(self):
        paths = sorted({self.model().filePath(i) for i in self.selectionModel().selectedRows(0)})
        # si un dossier est sélectionné, inutile de traiter aussi son contenu
        return [p for p in paths if not any(p.startswith(q + os.sep) for q in paths)]

    def target_dir(self, pos):
        idx = self.indexAt(pos)
        path = self.model().filePath(idx) if idx.isValid() else self.win.root
        return path if os.path.isdir(path) else os.path.dirname(path)

    def dragEnterEvent(self, e):
        if e.mimeData().hasUrls():
            e.acceptProposedAction()

    def dragMoveEvent(self, e):
        if not e.mimeData().hasUrls():
            return e.ignore()
        super().dragMoveEvent(e)  # défilement et ouverture auto des dossiers
        e.acceptProposedAction()

    def dropEvent(self, e):
        paths = [u.toLocalFile() for u in e.mimeData().urls() if u.isLocalFile()]
        dest = self.target_dir(e.position().toPoint())
        internal = e.source() is self
        # on gère nous-mêmes : la source ne doit rien supprimer ensuite
        e.setDropAction(Qt.DropAction.CopyAction)
        e.accept()
        self.win.move_paths(paths, dest, copy=not internal)

    def keyPressEvent(self, e):
        if e.key() == Qt.Key.Key_Delete:
            self.win.trash_paths(self.selected_paths())
        elif e.key() == Qt.Key.Key_F2 and len(self.selected_paths()) == 1:
            self.win.rename_path(self.selected_paths()[0])
        else:
            super().keyPressEvent(e)


AI_SITES = {  # assistants ouverts dans leur page de discussion
    "ChatGPT": "https://chatgpt.com/",
    "Claude": "https://claude.ai/new",
    "Gemini": "https://gemini.google.com/app",
    "Mistral (Le Chat)": "https://chat.mistral.ai/chat",
}
AI_PREPROMPT = (
    "Tu es un tuteur pour un étudiant en licence d'informatique. Aide-moi à comprendre et à "
    "progresser : explique clairement, en français, étape par étape. Quand il y a une erreur, "
    "montre où elle est et pourquoi elle se produit avant de proposer une correction. "
    "Donne des indices et des explications plutôt qu'une solution toute faite, sauf si je la "
    "demande explicitement. Appuie-toi sur le code et le contexte ci-dessous.")
AI_TASKS = {
    "explain": ("Expliquer ce code", "Explique-moi ce que fait ce code, partie par partie, et les notions utilisées."),
    "debug": ("Trouver l'erreur", "Mon programme ne fonctionne pas comme prévu. Aide-moi à trouver l'erreur à partir "
                                  "du code et de la sortie du programme."),
    "review": ("Relire et améliorer", "Relis mon code : dis-moi ce qui est bien, ce qui pourrait être plus clair ou "
                                      "plus correct, et pourquoi."),
    "test": ("Proposer des tests", "Propose des cas de test (et des doctests si c'est du Python) pour vérifier ce code, "
                                   "en expliquant ce que chacun vérifie."),
}
LANGS = {".py": "python", ".c": "c", ".h": "c", ".cpp": "cpp", ".java": "java", ".cs": "csharp", ".js": "javascript",
         ".sh": "bash", ".html": "html", ".css": "css", ".sql": "sql", ".json": "json", ".md": "markdown"}
IMG_EXT = (".png", ".jpg", ".jpeg", ".gif", ".bmp", ".webp", ".svg")
DEFAULT_FAVS = [["Mes cours URCA", "https://cours.univ-reims.fr/local/mycoursesurca/courses.php"],
                ["Bureau virtuel URCA", "https://ebureau.univ-reims.fr"],
                ["Site de l'URCA", "https://www.univ-reims.fr"],
                ["Documentation Python", "https://docs.python.org/fr/3/"]]


TAB_MIME = "application/x-studentcode-tab"


class PaneTabBar(QTabBar):
    """Onglets réordonnables ; sortir un onglet de la barre le fait glisser vers un autre
    volet ou vers un bord (pour scinder)."""

    def __init__(self, pane):
        super().__init__(pane)
        self.pane = pane
        self.press = None
        self.grabbed = -1

    def mousePressEvent(self, e):
        if e.button() == Qt.MouseButton.LeftButton:
            self.press = e.position().toPoint()
            self.grabbed = self.tabAt(self.press)
        super().mousePressEvent(e)

    def mouseReleaseEvent(self, e):
        self.press = None
        super().mouseReleaseEvent(e)

    def mouseMoveEvent(self, e):
        pos = e.position().toPoint()
        if self.press is not None and self.grabbed >= 0 and e.buttons() & Qt.MouseButton.LeftButton \
                and not self.rect().adjusted(-40, -14, 40, 14).contains(pos):
            w = self.pane.widget(self.grabbed)
            pm = self.grab(self.tabRect(self.grabbed))
            self.press = None
            # termine proprement le déplacement interne de QTabBar avant de lancer le glisser
            super().mouseReleaseEvent(QMouseEvent(QEvent.Type.MouseButtonRelease, e.position(),
                                                  e.globalPosition(), Qt.MouseButton.LeftButton,
                                                  Qt.MouseButton.NoButton, Qt.KeyboardModifier.NoModifier))
            if w is not None:
                self.pane.win.drag_tab(w, self, pm)
            return
        super().mouseMoveEvent(e)


class DropOverlay(QWidget):
    """Posé sur chaque volet pendant qu'on fait glisser un onglet : montre où il va atterrir."""

    def __init__(self, pane):
        super().__init__(pane)
        self.pane = pane
        self.zone = None
        self.setAcceptDrops(True)
        self.setGeometry(pane.rect())
        self.raise_()
        self.show()

    def zone_at(self, pos):
        w, h = max(self.width(), 1), max(self.height(), 1)
        x, y = pos.x() / w, pos.y() / h
        if x > 0.68:
            return "right"
        if x < 0.32:
            return "left"
        if y > 0.68:
            return "down"
        return "center"

    def zone_rect(self, zone):
        r = self.rect()
        w, h = r.width(), r.height()
        return {"right": r.adjusted(w // 2, 0, 0, 0), "left": r.adjusted(0, 0, -w // 2, 0),
                "down": r.adjusted(0, h // 2, 0, 0), "center": r}[zone]

    def dragEnterEvent(self, e):
        if e.mimeData().hasFormat(TAB_MIME):
            e.acceptProposedAction()
            self.dragMoveEvent(e)

    def dragMoveEvent(self, e):
        if not e.mimeData().hasFormat(TAB_MIME):
            return e.ignore()
        z = self.zone_at(e.position().toPoint())
        if z != self.zone:
            self.zone = z
            self.update()
        e.acceptProposedAction()

    def dragLeaveEvent(self, e):
        self.zone = None
        self.update()

    def dropEvent(self, e):
        self.pane.win.pending_drop = (self.pane, self.zone_at(e.position().toPoint()))
        e.acceptProposedAction()

    def paintEvent(self, e):
        if not self.zone:
            return
        p = QPainter(self)
        c = QColor(BLUE)
        c.setAlpha(60)
        r = self.zone_rect(self.zone).adjusted(3, 3, -3, -3)
        p.fillRect(r, c)
        p.setPen(QPen(QColor(BLUE), 2))
        p.drawRect(r)
        p.end()


class Pane(QTabWidget):
    """Un volet de la zone centrale : des onglets (code, web, PDF, image)."""

    def __init__(self, win):
        super().__init__()
        self.win = win
        self.setTabBar(PaneTabBar(self))
        self.setDocumentMode(True)
        self.setTabsClosable(True)
        self.setMovable(True)
        self.setProperty("active", "false")
        self.tabCloseRequested.connect(lambda i: win.close_widget(self.widget(i)))
        self.currentChanged.connect(lambda _: win.update_status())
        self.tabBar().setContextMenuPolicy(Qt.ContextMenuPolicy.CustomContextMenu)
        self.tabBar().customContextMenuRequested.connect(lambda pos: win.tab_menu(self, pos))

    def set_active(self, on):
        self.setProperty("active", "true" if on else "false")
        for w in (self, self.tabBar()):
            w.style().unpolish(w)
            w.style().polish(w)



class WebTab(QWidget):
    """Onglet navigateur (pages web et PDF), avec barre d'adresse et favoris."""

    def __init__(self, win, url=None, path=None):
        super().__init__()
        self.win = win
        self.path = path  # fichier local affiché (PDF), sinon None
        load_web()
        self.view = WebView(self)
        page = QWebEnginePage(win.web_profile(), self.view)
        # mise en veille quand l'onglet reste caché : le processus de la page est libéré,
        # elle se recharge toute seule quand on revient dessus
        self.sleep_timer = QTimer(self)
        self.sleep_timer.setSingleShot(True)
        self.sleep_timer.setInterval(int(win.settings.value("web_sleep_s", 60)) * 1000)
        self.sleep_timer.timeout.connect(self.sleep)
        self.view.setPage(page)
        st = self.view.settings()
        st.setAttribute(QWebEngineSettings.WebAttribute.PluginsEnabled, True)
        st.setAttribute(QWebEngineSettings.WebAttribute.PdfViewerEnabled, not HAS_PDF or bool(path))
        self.url_in = Line("Adresse web ou recherche…")
        self.url_in.keep = True
        self.url_in.submitted.connect(self.load)
        self.url_in.escaped.connect(self.view.setFocus)
        bar = QHBoxLayout()
        bar.setContentsMargins(6, 4, 6, 4)
        bar.setSpacing(2)

        def btn(name, tip, fn, col=FG):
            b = QToolButton()
            b.setIcon(icon(name, col))
            b.setToolTip(tip)
            b.clicked.connect(fn)
            bar.addWidget(b)
            return b
        btn("back", "Page précédente", self.view.back)
        btn("forward", "Page suivante", self.view.forward)
        btn("reload", "Recharger", self.view.reload)
        bar.addWidget(self.url_in, 1)
        self.star = btn("star", "Ajouter / retirer des favoris", self.toggle_fav, YELLOW)
        btn("external", "Ouvrir dans le navigateur du système",
            lambda: QDesktopServices.openUrl(self.view.url()), COMMENT)
        lay = QVBoxLayout(self)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.setSpacing(0)
        lay.addLayout(bar)
        lay.addWidget(self.view, 1)
        self.view.urlChanged.connect(self.url_changed)
        self.view.titleChanged.connect(lambda _: win.retitle_tab(self))
        self.setFocusProxy(self.view)
        if path:
            self.view.setUrl(QUrl.fromLocalFile(path))
        elif url:
            self.load(url)

    def title(self):
        if self.path:
            return os.path.basename(self.path)
        t = self.view.title() or self.view.url().host() or "Nouvel onglet"
        return t if len(t) <= 26 else t[:25] + "…"

    def load(self, text):
        text = text.strip()
        if not text:
            return
        if "://" in text or text.startswith(("about:", "file:", "data:")):
            url = QUrl(text)
        elif os.path.exists(os.path.expanduser(text)):
            url = QUrl.fromLocalFile(os.path.abspath(os.path.expanduser(text)))
        elif "." in text and " " not in text:
            url = QUrl("https://" + text)
        else:
            url = QUrl("https://duckduckgo.com/?q=" + quote_plus(text))
        self.view.setUrl(url)
        self.view.setFocus()

    def hideEvent(self, e):
        super().hideEvent(e)
        if self.sleep_timer.interval() > 0:
            self.sleep_timer.start()

    def showEvent(self, e):
        super().showEvent(e)
        self.sleep_timer.stop()
        wake(self.view.page())

    def sleep(self):
        if not self.isVisible():
            discard(self.view.page())

    def url_changed(self, url):
        self.url_in.setText(url.toString())
        self.url_in.setCursorPosition(0)
        is_fav = any(u == url.toString() for _, u in self.win.favs())
        self.star.setIcon(icon("star", YELLOW if is_fav else COMMENT))

    def toggle_fav(self):
        url = self.view.url().toString()
        if not url or url == "about:blank":
            return
        favs = self.win.favs()
        if any(u == url for _, u in favs):
            favs = [f for f in favs if f[1] != url]
            msg = "Retiré des favoris"
        else:
            favs.append([self.view.title() or url, url])
            msg = "Ajouté aux favoris"
        self.win.set_favs(favs)
        self.url_changed(self.view.url())
        self.win.statusBar().showMessage(msg, 2500)


class ImageTab(QScrollArea):
    def __init__(self, win, path):
        super().__init__()
        self.path = path
        self.pm = QPixmap(path)
        self.lbl = QLabel()
        self.lbl.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.setWidget(self.lbl)
        self.setWidgetResizable(True)
        self.setFrameShape(QScrollArea.Shape.NoFrame)

    def title(self):
        return os.path.basename(self.path)

    def resizeEvent(self, e):
        super().resizeEvent(e)
        if not self.pm.isNull():
            size = self.viewport().size()
            pm = self.pm
            if pm.width() > size.width() or pm.height() > size.height():
                pm = pm.scaled(size, Qt.AspectRatioMode.KeepAspectRatio,
                               Qt.TransformationMode.SmoothTransformation)
            self.lbl.setPixmap(pm)


class StartTab(QWidget):
    """Page d'accueil d'un volet : nouveau fichier, ouvrir, web, favoris."""
    path = None

    def __init__(self, win):
        super().__init__()
        self.win = win
        col = QVBoxLayout()
        col.setSpacing(8)
        # titre aux couleurs du logo : « Student » en blanc, « Code » en bleu, quel que soit le thème
        white = FG if theme.LIGHT else "#ffffff"
        big = QLabel(f'<span style="color:{white}">Student</span><span style="color:#2e7de9">Code</span>')
        big.setObjectName("big")
        big.setTextFormat(Qt.TextFormat.RichText)
        logo = QLabel()
        logo.setPixmap(QIcon(os.path.join(APP_DIR, "icon.svg")).pixmap(QSize(46, 46)))
        head = QHBoxLayout()
        head.setSpacing(12)
        head.addWidget(logo)
        head.addWidget(big)
        head.addStretch(1)
        col.addLayout(head)
        col.addSpacing(10)

        def link(ic, colr, text, fn):
            b = QPushButton("  " + text)
            b.setIcon(icon(ic, colr))
            b.setObjectName("link")
            b.setCursor(Qt.CursorShape.PointingHandCursor)
            b.clicked.connect(fn)
            col.addWidget(b)
        link("new", FG, "Nouveau fichier Python", lambda: win.new_file())
        link("open", YELLOW, "Ouvrir un fichier (code, PDF, image)…", win.open_dialog)
        link("open", YELLOW, "Ouvrir un dossier de projet…", win.open_folder)
        col.addSpacing(10)
        self.url_in = Line("Adresse web ou recherche, puis Entrée…")
        self.url_in.submitted.connect(lambda t: win.open_url(t) if t.strip() else None)
        col.addWidget(self.url_in)
        for title, url in win.favs()[:8]:
            link("globe", CYAN, title, lambda _=False, u=url: win.open_url(u))
        col.addStretch(1)
        box = QWidget()
        box.setLayout(col)
        box.setFixedWidth(420)
        outer = QHBoxLayout(self)
        outer.addStretch(1)
        v = QVBoxLayout()
        v.addStretch(1)
        v.addWidget(box)
        v.addStretch(2)
        outer.addLayout(v)
        outer.addStretch(1)
        self.setFocusProxy(self.url_in)
        self.setStyleSheet(f"StartTab {{ background: {BG}; }}")

    def title(self):
        return "Accueil"


# =========================================================== fenêtre principale
class MainWindow(QMainWindow):
    def __init__(self, args):
        super().__init__()
        self.settings = QSettings("StudentCode", "StudentCode")
        self.vim_on = self.settings.value("vim", False, type=bool)
        self.vim_linewise = False
        self.scrollbars = self.settings.value("scrollbars", False, type=bool)  # masquées par défaut
        self.rrun = None  # connexion SSH sur laquelle un programme tourne, sinon None
        self._errbuf = {}
        self.frameless = self.settings.value("frameless", True, type=bool)
        if self.frameless:
            self.setWindowFlag(Qt.WindowType.FramelessWindowHint, True)
        f = QFontDatabase.systemFont(QFontDatabase.SystemFont.FixedFont)
        for fam in ("JetBrainsMono Nerd Font", "JetBrains Mono", "FiraCode Nerd Font",
                    "Fira Code", "Cascadia Mono", "Ubuntu Mono", "DejaVu Sans Mono", "Consolas"):
            if fam in QFontDatabase.families():
                f = QFont(fam)
                break
        f.setStyleHint(QFont.StyleHint.Monospace)
        f.setPointSize(int(self.settings.value("fontsize", 11)))
        self.code_font = f
        self.proc = self.server = self.dsock = None
        self.dbuf = b""
        self.state = "idle"
        self.stack = []
        self.frame = 0
        self.child_req = {}
        self.req_id = 0
        self.run_args = []
        self.untitled = 0
        self.setWindowTitle("StudentCode")
        self.setWindowIcon(QIcon(os.path.join(APP_DIR, "icon.svg")))
        self.resize(1360, 820)

        folder = None
        files = []
        for a in args:
            if os.path.isdir(a):
                folder = os.path.abspath(a)
            else:
                files.append(os.path.abspath(a))
        self.root = folder or self.settings.value("root", "") or os.path.expanduser("~")
        if not os.path.isdir(self.root):
            self.root = os.path.expanduser("~")

        self.build_ui()
        self.build_actions()
        geo = self.settings.value("geometry")
        if geo:
            self.restoreGeometry(geo)
        restored = False
        if not files and not folder:
            restored = self.restore_layout()
            if not restored:
                files = [p for p in (self.settings.value("files", []) or []) if os.path.isfile(p)]
        for p in files:
            self.open_file(p)
        if not self.tabs.count():
            if not self.settings.value("seen", False, type=bool):
                self.new_file(DEMO)
            else:
                self.add_tab(StartTab(self))
            self.settings.setValue("seen", True)
        QApplication.instance().focusChanged.connect(self.on_focus)
        QTimer.singleShot(0, self.restore_tree)
        self.set_state("idle")
        self.apply_scrollbars()
        self.repl_proc = None
        self.bottom.currentChanged.connect(
            lambda _i: self.start_repl() if self.bottom.currentWidget() is self.repl and self.repl_proc is None else None)

    # ------------------------------------------------------------------ UI
    def build_ui(self):
        # --- explorateur
        self.fs = QFileSystemModel(self)
        self.fs.setReadOnly(False)
        self.icon_provider = IconProvider()
        self.fs.setIconProvider(self.icon_provider)
        self.tree = FileTree(self)
        self.tree.setModel(self.fs)
        self.tree.setHeaderHidden(True)
        for c in (1, 2, 3):
            self.tree.hideColumn(c)
        self.tree.setIndentation(14)
        self.tree.activated.connect(self.tree_open)
        self.tree.clicked.connect(lambda i: self.tree_open(i) if QApplication.keyboardModifiers() == Qt.KeyboardModifier.NoModifier else None)
        self.tree.setContextMenuPolicy(Qt.ContextMenuPolicy.CustomContextMenu)
        self.tree.customContextMenuRequested.connect(self.tree_menu)
        self.open_dirs = set()
        self._restoring_tree = False
        self.tree.expanded.connect(lambda i: self.tree_expanded(i, True))
        self.tree.collapsed.connect(lambda i: self.tree_expanded(i, False))
        self.root_lbl = QLabel()
        self.root_lbl.setObjectName("title")
        files_w = QWidget()
        ll = QVBoxLayout(files_w)
        ll.setContentsMargins(0, 0, 0, 0)
        ll.setSpacing(0)
        ll.addWidget(self.root_lbl)
        ll.addWidget(self.tree)
        self.left = QTabWidget()
        self.left.setDocumentMode(True)
        self.left.addTab(files_w, icon("folder", BLUE, size=15), "Fichiers")
        self.left.setUsesScrollButtons(False)
        self.left.setStyleSheet("QTabBar::tab { padding: 6px 7px; }")
        self.left.setMinimumWidth(215)
        self.set_root(self.root)

        # --- éditeur à onglets + barre de recherche + ligne de commande vim
        self.panes = []
        self.active = None
        self.last_editor = None
        self._profile = None
        self.area = QSplitter(Qt.Orientation.Horizontal)  # racine des volets
        self.area.addWidget(self.new_pane())
        self.set_active(self.panes[0])
        self.find_in = Line("Rechercher")
        self.find_in.keep = True
        self.repl_in = Line("Remplacer par")
        self.repl_in.keep = True
        self.find_in.submitted.connect(lambda _: self.find_next(True))
        self.find_in.escaped.connect(self.hide_find)
        self.repl_in.submitted.connect(lambda _: self.replace_one())
        self.repl_in.escaped.connect(self.hide_find)
        self.findbar = QWidget()
        fl = QHBoxLayout(self.findbar)
        fl.setContentsMargins(6, 4, 6, 4)
        fl.addWidget(self.find_in, 2)
        for text, fn in (("↑", lambda: self.find_next(False)), ("↓", lambda: self.find_next(True))):
            b = QPushButton(text)
            b.clicked.connect(fn)
            fl.addWidget(b)
        fl.addWidget(self.repl_in, 2)
        for text, fn in (("Remplacer", self.replace_one), ("Tout", self.replace_all),
                         ("✕", self.hide_find)):
            b = QPushButton(text)
            b.clicked.connect(fn)
            fl.addWidget(b)
        self.findbar.hide()
        self.cmdline = Line(":")
        self.cmdline.setFont(self.code_font)
        self.cmdline.submitted.connect(self.vim_command)
        self.cmdline.escaped.connect(self.hide_cmdline)
        self.cmdline.hide()
        center = QWidget()
        cl = QVBoxLayout(center)
        cl.setContentsMargins(0, 0, 0, 0)
        cl.setSpacing(0)
        cl.addWidget(self.area, 1)
        cl.addWidget(self.findbar)
        cl.addWidget(self.cmdline)

        # --- panneau du bas : sortie + interpréteur
        self.out = Console(self, STDIN_HINT)
        self.out.submitted.connect(self.send_stdin)
        self.repl = Console(self, ">>> tape du Python ici")
        self.repl.submitted.connect(self.repl_send)
        self.bottom = QTabWidget()
        self.bottom.setDocumentMode(True)
        self.bottom.addTab(self.out, icon("output", GREEN, size=15), "Sortie  ")
        self.bottom.addTab(self.repl, file_icon("x.py"), "Interpréteur Python  ")
        self.terminal = LocalTerminal(self)
        self.bottom.addTab(self.terminal, icon("terminal", GREEN, size=15), "Terminal  ")
        self.vpn_panel = VpnPanel(self)
        self.ssh_hub = RemoteHub(self)   # plusieurs machines en même temps, un terminal chacune
        # clic droit sur un onglet du bas : « Nouveau terminal » sur la même machine
        self.bottom.tabBar().setContextMenuPolicy(Qt.ContextMenuPolicy.CustomContextMenu)
        self.bottom.tabBar().customContextMenuRequested.connect(self.bottom_menu)
        self.left.addTab(self.ssh_hub, icon("server", CYAN, size=15), "SSH")
        self.left.addTab(self.vpn_panel, icon("shield", GREEN, size=15), "VPN")
        # onglets des panneaux réordonnables à la souris ; l'ordre est retenu
        for name, w in (("out", self.out), ("repl", self.repl), ("term", self.terminal),
                        ("files", files_w), ("ssh", self.ssh_hub), ("vpn", self.vpn_panel)):
            w.setObjectName(name)
        for key, tabs in (("bottom_order", self.bottom), ("left_order", self.left)):
            tabs.setMovable(True)
            order = self.settings.value(key, []) or []
            for pos, name in enumerate(order if isinstance(order, list) else []):
                i = next((j for j in range(tabs.count()) if tabs.widget(j).objectName() == name), -1)
                if i >= 0 and pos < tabs.count():
                    tabs.tabBar().moveTab(i, pos)
            tabs.setCurrentIndex(0)
        self.bottom.setCurrentWidget(self.terminal)   # le terminal est l'onglet affiché par défaut

        # --- panneau de droite : débogueur
        self.vars = QTreeWidget()
        self.vars.setHeaderLabels(["Nom", "Type", "Valeur"])
        self.vars.setColumnWidth(0, 130)
        self.vars.setColumnWidth(1, 90)
        self.vars.setFont(self.code_font)
        self.vars.itemExpanded.connect(self.var_expanded)
        self.stack_list = QListWidget()
        self.stack_list.setFont(self.code_font)
        self.stack_list.currentRowChanged.connect(self.select_frame)
        self.eval_in = Line("Évaluer une expression à l'arrêt (ex : len(suite))")
        self.eval_in.setFont(self.code_font)
        self.eval_in.submitted.connect(self.debug_eval)
        self.right = QWidget()
        rl = QVBoxLayout(self.right)
        rl.setContentsMargins(0, 0, 0, 0)
        rl.setSpacing(0)
        def titled(name, text):
            box = QWidget()
            h = QHBoxLayout(box)
            h.setContentsMargins(10, 6, 6, 4)
            h.setSpacing(6)
            pic = QLabel()
            pic.setPixmap(pixmap(name, COMMENT, 15))
            lab = QLabel(text)
            lab.setStyleSheet(f"color: {COMMENT}; font-weight: 700;")
            h.addWidget(pic)
            h.addWidget(lab, 1)
            return box
        t1 = titled("debug", "VARIABLES")
        t2 = titled("output", "PILE D'APPELS")
        rsplit = QSplitter(Qt.Orientation.Vertical)
        top = QWidget()
        tl = QVBoxLayout(top)
        tl.setContentsMargins(0, 0, 0, 0)
        tl.setSpacing(0)
        tl.addWidget(t1)
        tl.addWidget(self.vars)
        tl.addWidget(self.eval_in)
        bot = QWidget()
        bl = QVBoxLayout(bot)
        bl.setContentsMargins(0, 0, 0, 0)
        bl.setSpacing(0)
        bl.addWidget(t2)
        bl.addWidget(self.stack_list)
        rsplit.addWidget(top)
        rsplit.addWidget(bot)
        rsplit.setSizes([520, 200])
        rl.addWidget(rsplit)

        # --- assemblage
        self.vsplit = QSplitter(Qt.Orientation.Vertical)
        self.vsplit.addWidget(center)
        self.vsplit.addWidget(self.bottom)
        self.vsplit.setSizes([560, 220])
        self.hsplit = QSplitter(Qt.Orientation.Horizontal)
        self.hsplit.addWidget(self.left)
        self.hsplit.addWidget(self.vsplit)
        self.hsplit.addWidget(self.right)
        self.ai_panel = AiPanel(self, AI_TASKS, AI_PREPROMPT, AI_SITES)
        self.hsplit.addWidget(self.ai_panel)
        self.ai_panel.hide()
        self.hsplit.setSizes([255, 790, 340])
        self.right.hide()  # le panneau de débogage s'affiche quand on débogue
        self.hsplit.setStretchFactor(1, 1)
        self.setCentralWidget(self.hsplit)

        # --- barre d'état façon lualine
        sb = self.statusBar()
        sb.setSizeGripEnabled(False)
        self.mode_lbl = QLabel()
        self.state_lbl = QLabel()
        self.file_lbl = QLabel()
        self.pos_lbl = QLabel()
        venv = " (venv)" if sys.prefix != sys.base_prefix else ""
        self.py_lbl = QLabel(f"  Python {sys.version.split()[0]}{venv}  ")
        self.py_lbl.setStyleSheet(f"background: {BG_HL}; color: {BLUE}; padding: 3px 4px;")
        sb.addWidget(self.mode_lbl)
        sb.addWidget(self.state_lbl)
        sb.addWidget(self.file_lbl)
        sb.addPermanentWidget(self.pos_lbl)
        sb.addPermanentWidget(self.py_lbl)

    def build_actions(self):
        def act(text, fn, key=None, tip=None, checkable=False):
            a = QAction(text, self)
            if key:
                a.setShortcuts([QKeySequence(k) for k in ([key] if isinstance(key, str) else key)])
            a.triggered.connect(fn)
            if tip or key:
                k = key if isinstance(key, str) else (key[0] if key else "")
                a.setToolTip(f"{tip or text.strip()}  ({k})" if k else (tip or text))
            a.setCheckable(checkable)
            self.addAction(a)
            return a

        A = self.a = {}
        A["new"] = act("Nouveau fichier", lambda: self.new_file(), "Ctrl+N")
        A["open"] = act("Ouvrir un fichier…", self.open_dialog, "Ctrl+O")
        A["folder"] = act("Ouvrir un dossier…", self.open_folder, "Ctrl+Shift+O")
        A["save"] = act("Enregistrer", lambda: self.save(), "Ctrl+S")
        A["saveas"] = act("Enregistrer sous…", lambda: self.save_as(), "Ctrl+Shift+S")
        A["close"] = act("Fermer l'onglet", lambda: self.close_tab(self.tabs.currentIndex()), "Ctrl+W")
        A["quit"] = act("Quitter", self.close, "Ctrl+Q")
        A["finder"] = act("Rechercher un fichier…", self.show_finder, "Ctrl+P")
        A["find"] = act("Rechercher / Remplacer", self.show_find, "Ctrl+F")
        A["goto"] = act("Aller à la ligne…", self.goto_dialog, "Ctrl+G")
        A["comment"] = act("Commenter / décommenter", lambda: self.cur() and self.cur().toggle_comment(), "Ctrl+/")
        A["run"] = act("Exécuter", lambda: self.start(False), "F5", "Exécuter le fichier courant")
        A["debug"] = act("Déboguer", self.debug_or_continue, "F6",
                         "Déboguer jusqu'au prochain point d'arrêt / Continuer")
        A["over"] = act("Pas à pas", lambda: self.step("next"), "F10", "Exécuter la ligne (par-dessus)")
        A["into"] = act("Entrer", lambda: self.step("step"), "F11", "Entrer dans la fonction")
        A["outof"] = act("Sortir", lambda: self.step("return"), "Shift+F11", "Sortir de la fonction")
        A["pause"] = act("Pause", lambda: self.dbg_send({"cmd": "pause"}), None, "Mettre en pause")
        A["stop"] = act("Arrêter", self.stop, "Shift+F5", "Arrêter le programme")
        A["bp"] = act("Point d'arrêt", self.toggle_bp, "F9", "Poser / enlever un point d'arrêt")
        A["bpclear"] = act("Supprimer tous les points d'arrêt", self.clear_bps)
        A["args"] = act("Arguments du programme…", self.ask_args)
        A["libs"] = act("Bibliothèques", lambda: Packages(self).exec(), "Ctrl+Shift+L",
                        "Installer / gérer les bibliothèques (pip)")
        A["repl_reset"] = act("Redémarrer l'interpréteur", self.start_repl)
        A["explorer"] = act("Explorateur", self.toggle_explorer, "Ctrl+B")
        A["panel"] = act("Panneau du bas", lambda: self.bottom.setVisible(not self.bottom.isVisible()), "Ctrl+J")
        A["dbgpanel"] = act("Panneau de débogage", lambda: self.right.setVisible(not self.right.isVisible()), "Ctrl+Shift+D")
        A["vim"] = act("Vim", self.toggle_vim, "Ctrl+Alt+V", "Activer / désactiver le mode Vim", True)
        A["vim"].setChecked(self.vim_on)
        A["nexttab"] = act("Onglet suivant", lambda: self.next_tab(1), "Ctrl+Tab")
        A["prevtab"] = act("Onglet précédent", lambda: self.next_tab(-1), "Ctrl+Shift+Tab")
        A["help"] = act("Raccourcis clavier", self.show_help, "F1")
        A["ssh"] = act("Connexion SSH (labs)", self.show_ssh, "Ctrl+Alt+S",
                       "Se connecter à une machine en SSH")
        A["vpn"] = act("VPN", lambda: (self.left.show(), self.left.setCurrentWidget(self.vpn_panel)),
                       None, "Gérer les VPN (fichiers .ovpn)")
        A["term"] = act("Terminal", self.show_terminal, "Alt+T", "Ouvrir le terminal")
        A["gotodef"] = act("Aller à la définition", lambda: self.cur() and self.cur().goto_definition(), "F12")

        A["splitr"] = act("Scinder à droite", lambda: self.split(Qt.Orientation.Horizontal), "Alt+V",
                          "Nouveau volet à droite")
        A["splitd"] = act("Scinder en bas", lambda: self.split(Qt.Orientation.Vertical), "Alt+H",
                          "Nouveau volet en bas")
        A["web"] = act("Nouvel onglet web", lambda: self.open_url(None), "Ctrl+T",
                       "Ouvrir une page web (cours, documentation…)")
        A["nextpane"] = act("Volet suivant", lambda: self.next_pane(1), "Alt+W")
        A["movetab"] = act("Déplacer l'onglet vers le volet suivant", self.move_tab_next, "Alt+Shift+W")
        A["closepane"] = act("Fermer le volet", self.close_pane, "Alt+Q")
        A["sysbar"] = act("Barre de titre du système", self.toggle_frameless, None, None, True)
        A["scrollbars"] = act("Barres de défilement", self.toggle_scrollbars, None,
                              "Afficher les barres de défilement (la molette fait défiler dans tous les cas)", True)
        A["scrollbars"].setChecked(self.scrollbars)
        A["webgpu"] = act("Accélération graphique des pages web", self.toggle_webgpu, None,
                          "Plus fluide sur les sites lourds, mais ~70 Mo de mémoire en plus (au prochain lancement)", True)
        A["webgpu"].setChecked(self.settings.value("web_gpu", False, type=bool))
        self.theme_menu = QMenu("Thème", self)
        self.theme_menu.setIcon(icon("palette", MAGENTA))
        for name in theme.THEMES:
            a = self.theme_menu.addAction(name)
            a.setCheckable(True)
            a.setChecked(name == theme.NAME)
            a.triggered.connect(lambda _=False, n=name: self.choose_theme(n))
        A["sysbar"].setChecked(not self.frameless)
        self.menuBar().hide()
        self.main_menu = QMenu(self)
        for title, names in (
            ("&Fichier", ["new", "open", "folder", None, "save", "saveas", None, "close", "quit"]),
            ("&Édition", ["find", "goto", "gotodef", "comment", None, "finder"]),
            ("E&xécution", ["run", "debug", "stop", None, "over", "into", "outof", "pause", None,
                            "bp", "bpclear", None, "args"]),
            ("&Outils", ["libs", "repl_reset", None, "ssh", "vpn"]),
            ("&Volets", ["splitr", "splitd", "closepane", None, "nextpane", "movetab", None, "web"]),
            ("&Affichage", ["explorer", "panel", "dbgpanel", "term", None, "vim", "sysbar", "scrollbars", "webgpu", None,
                            "nexttab", "prevtab"]),
            ("A&ide", ["help"])):
            m = self.main_menu.addMenu(title)
            for n in names:
                m.addSeparator() if n is None else m.addAction(A[n])
            if title == "&Affichage":
                m.addSeparator()
                m.addMenu(self.theme_menu)

        tb = self.bar = QToolBar("Barre")
        tb.setMovable(False)
        tb.setFloatable(False)
        tb.setContextMenuPolicy(Qt.ContextMenuPolicy.PreventContextMenu)
        tb.setToolButtonStyle(Qt.ToolButtonStyle.ToolButtonIconOnly)
        tb.setIconSize(QSize(18, 18))
        self.addToolBar(tb)
        mbtn = QToolButton()
        mbtn.setIcon(icon("menu", FG))
        mbtn.setToolTip("Menu  (Alt+M)")
        mbtn.setShortcut(QKeySequence("Alt+M"))
        mbtn.setMenu(self.main_menu)
        mbtn.setPopupMode(QToolButton.ToolButtonPopupMode.InstantPopup)
        mbtn.setStyleSheet("QToolButton::menu-indicator { image: none; }")
        mbtn.clicked.connect(mbtn.showMenu)
        tb.addWidget(mbtn)
        tb.addSeparator()
        self.tb_proxies = {}
        self._tb_shown = None
        for n, obj in (("new", ""), ("open", ""), ("save", ""), (None, ""), ("run", "run"),
                       ("debug", "debug"), ("over", "step"), ("into", "step"), ("outof", "step"),
                       ("pause", "step"), ("stop", "stop"), ("bp", "stop"),
                       (None, ""), ("libs", "libs"), ("finder", ""), ("vim", ""),
                       (None, ""), ("splitr", ""), ("splitd", ""), ("web", ""), ("ssh", ""), ("vpn", "")):
            if n is None:
                tb.addSeparator()
                continue
            if n in self.CONTEXT_BUTTONS:
                a = self.toolbar_proxy(A[n])
                self.tb_proxies[n] = a
            else:
                a = A[n]
            tb.addAction(a)
            if obj:
                tb.widgetForAction(a).setObjectName(obj)
        # zone de titre : sert aussi à déplacer la fenêtre
        self.title_lbl = QLabel("StudentCode")
        self.title_lbl.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.title_lbl.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Preferred)
        self.title_lbl.setStyleSheet(f"color: {COMMENT}; background: transparent;")
        tb.addWidget(self.title_lbl)
        self.ai_btn = QToolButton()
        self.ai_btn.setIcon(icon("spark", MAGENTA))
        self.ai_btn.setText(" IA")
        self.ai_btn.setToolButtonStyle(Qt.ToolButtonStyle.ToolButtonTextBesideIcon)
        self.ai_btn.setToolTip("Aide par IA sur le fichier courant  (Alt+I)")
        self.ai_btn.setCheckable(True)
        self.ai_btn.setStyleSheet(f"QToolButton {{ color: {MAGENTA}; }}")
        self.ai_btn.setShortcut(QKeySequence("Alt+I"))
        self.ai_btn.clicked.connect(self.toggle_ai)
        tb.addWidget(self.ai_btn)
        self.win_btns = []
        for name, col, tip, fn in (("min", FG, "Réduire", self.showMinimized),
                                   ("max", FG, "Agrandir / restaurer", self.toggle_max),
                                   ("close", RED, "Fermer", self.close)):
            b = QToolButton()
            b.setIcon(icon(name, col))
            b.setToolTip(tip)
            b.clicked.connect(fn)
            self.win_btns.append(tb.addWidget(b))
        tb.installEventFilter(self)
        self.title_lbl.installEventFilter(self)
        for wdg in (tb, self.statusBar(), self.centralWidget()):
            wdg.setCursor(Qt.CursorShape.ArrowCursor)
        self.setMouseTracking(True)
        self.apply_frameless()
        A["new"].setText("Nouveau")
        A["open"].setText("Ouvrir")
        A["finder"].setText("Fichiers")
        for n, (ic, col) in {"new": ("new", FG), "open": ("open", YELLOW), "save": ("save", BLUE),
                             "run": ("run", GREEN), "debug": ("debug", ORANGE),
                             "over": ("over", BLUE), "into": ("into", BLUE),
                             "outof": ("outof", BLUE), "pause": ("pause", BLUE),
                             "stop": ("stop", RED), "bp": ("bp", RED), "libs": ("libs", MAGENTA),
                             "finder": ("search", CYAN), "vim": ("vim", GREEN),
                             "folder": ("open", YELLOW), "find": ("search", CYAN),
                             "saveas": ("save", BLUE), "close": ("close", COMMENT),
                             "splitr": ("split_r", BLUE), "splitd": ("split_d", BLUE),
                             "web": ("globe", CYAN), "closepane": ("close", COMMENT), "ssh": ("server", CYAN), "vpn": ("shield", GREEN), "term": ("terminal", GREEN),
                             "repl_reset": ("reload", GREEN)}.items():
            A[n].setIcon(icon(ic, col))

    # ------------------------------------------------- fenêtre sans bordure
    def apply_frameless(self):
        for a in self.win_btns:
            a.setVisible(self.frameless)
        self.statusBar().setSizeGripEnabled(self.frameless)
        m = 5 if self.frameless and not self.isMaximized() else 0
        self.setContentsMargins(m, m, m, m)

    def toggle_frameless(self):
        self.frameless = not self.a["sysbar"].isChecked()
        self.setWindowFlag(Qt.WindowType.FramelessWindowHint, self.frameless)
        self.apply_frameless()
        self.show()

    def toggle_max(self):
        self.showNormal() if self.isMaximized() else self.showMaximized()

    def changeEvent(self, e):
        if e.type() == QEvent.Type.WindowStateChange and hasattr(self, "win_btns"):
            self.apply_frameless()
        super().changeEvent(e)

    def eventFilter(self, obj, ev):
        # glisser la barre = déplacer la fenêtre ; double-clic = agrandir
        if self.frameless and ev.type() == QEvent.Type.MouseButtonPress \
                and ev.button() == Qt.MouseButton.LeftButton and self.windowHandle():
            self.windowHandle().startSystemMove()
            return True
        if self.frameless and ev.type() == QEvent.Type.MouseButtonDblClick:
            self.toggle_max()
            return True
        return super().eventFilter(obj, ev)

    def edges_at(self, pos):
        if not self.frameless or self.isMaximized():
            return None
        m, r, E = 6, self.rect(), Qt.Edge
        parts = [edge for cond, edge in ((pos.x() <= m, E.LeftEdge), (pos.x() >= r.width() - m, E.RightEdge),
                                         (pos.y() <= m, E.TopEdge), (pos.y() >= r.height() - m, E.BottomEdge))
                 if cond]
        if not parts:
            return None
        edges = parts[0]
        for p in parts[1:]:
            edges |= p
        return edges

    def mouseMoveEvent(self, e):
        edges = self.edges_at(e.position().toPoint())
        E, C = Qt.Edge, Qt.CursorShape
        if edges is None:
            self.unsetCursor()
        elif edges in (E.LeftEdge, E.RightEdge):
            self.setCursor(C.SizeHorCursor)
        elif edges in (E.TopEdge, E.BottomEdge):
            self.setCursor(C.SizeVerCursor)
        elif edges in (E.LeftEdge | E.TopEdge, E.RightEdge | E.BottomEdge):
            self.setCursor(C.SizeFDiagCursor)
        else:
            self.setCursor(C.SizeBDiagCursor)
        super().mouseMoveEvent(e)

    def mousePressEvent(self, e):
        edges = self.edges_at(e.position().toPoint())
        if edges is not None and e.button() == Qt.MouseButton.LeftButton and self.windowHandle():
            self.windowHandle().startSystemResize(edges)
            return
        super().mousePressEvent(e)

    # --------------------------------------------------------------- état
    @property
    def ssh_panel(self):
        """La connexion SSH affichée (il peut y en avoir plusieurs)."""
        return self.ssh_hub.current_panel()

    @property
    def ssh_console(self):
        return self.ssh_panel.console

    @property
    def tabs(self):
        """Le volet actif (un QTabWidget)."""
        return self.active

    def all_tabs(self):
        return [p.widget(i) for p in self.panes for i in range(p.count())]

    def editors(self):
        return [w for w in self.all_tabs() if isinstance(w, Editor)]

    def cur(self):
        """L'éditeur courant ; si le volet actif montre une page web ou un PDF,
        on garde le dernier éditeur utilisé (pour F5, le débogage, etc.)."""
        w = self.tabs.currentWidget()
        if isinstance(w, Editor):
            return w
        if self.last_editor is not None and self.last_editor in self.editors():
            return self.last_editor
        return None

    # --------------------------------------------------------------- volets
    def new_pane(self):
        p = Pane(self)
        self.panes.append(p)
        return p

    def pane_of(self, w):
        return next((p for p in self.panes if p.indexOf(w) >= 0), None)

    def set_active(self, pane):
        if pane is None or pane not in self.panes:
            return
        self.active = pane
        for p in self.panes:
            p.set_active(p is pane and len(self.panes) > 1)
        self.update_status()

    def on_focus(self, old, now):
        w = now
        while w is not None:
            if isinstance(w, Editor):
                self.last_editor = w
            if isinstance(w, Pane):
                if w is not self.active:
                    self.set_active(w)
                return
            w = w.parentWidget()

    def add_tab(self, w, pane=None, ic=None, title=None):
        from PyQt6.QtWidgets import QAbstractScrollArea
        for area in [w] * isinstance(w, QAbstractScrollArea) + w.findChildren(QAbstractScrollArea):
            self.style_scrollbars(area)
        pane = pane if pane in self.panes else self.active
        if ic is None:
            ic = icon("globe", CYAN) if isinstance(w, WebTab) and not w.path else \
                 icon("new", COMMENT) if isinstance(w, StartTab) else file_icon(w.path or "x.py")
        i = pane.addTab(w, ic, title or w.title())
        pane.setCurrentIndex(i)
        # la page d'accueil laisse la place au premier vrai onglet
        if not isinstance(w, StartTab):
            for j in reversed(range(pane.count())):
                st = pane.widget(j)
                if isinstance(st, StartTab):
                    pane.removeTab(j)
                    st.deleteLater()
        self.set_active(pane)
        w.setFocus()
        return w

    def retitle_tab(self, w):
        pane = self.pane_of(w)
        if pane:
            i = pane.indexOf(w)
            pane.setTabText(i, w.title())
            pane.setTabToolTip(i, w.path or (w.view.url().toString() if isinstance(w, WebTab) else ""))
            self.update_status()

    def drag_tab(self, w, source, pixmap):
        """Fait glisser l'onglet `w` : dépôt au centre d'un volet = déplacer,
        sur un bord = scinder de ce côté."""
        self.pending_drop = None
        overlays = [DropOverlay(p) for p in self.panes]
        drag = QDrag(source)
        mime = QMimeData()
        mime.setData(TAB_MIME, b"1")
        drag.setMimeData(mime)
        drag.setPixmap(pixmap)
        drag.exec(Qt.DropAction.MoveAction)
        for o in overlays:
            o.setParent(None)
            o.deleteLater()
        if self.pending_drop:
            pane, zone = self.pending_drop
            self.pending_drop = None
            self.drop_tab(w, pane, zone)

    def drop_tab(self, w, target, zone):
        src = self.pane_of(w)
        if src is None or target not in self.panes:
            return
        if zone == "center":
            if target is not src:
                self.move_tab(w, target)
            return
        if src is target and src.count() == 1:
            return  # seul onglet du volet : rien à scinder
        self.set_active(target)
        o = Qt.Orientation.Vertical if zone == "down" else Qt.Orientation.Horizontal
        self.move_tab(w, self.split(o, start=False, before=zone == "left"))

    def split(self, orientation, start=True, before=False):
        """Scinde le volet actif ; renvoie le nouveau volet."""
        src = self.active
        new = self.new_pane()
        sp = src.parentWidget()
        idx = sp.indexOf(src)
        if sp.count() == 1:
            sp.setOrientation(orientation)
        if sp.orientation() == orientation:
            sp.insertWidget(idx if before else idx + 1, new)
            sp.setSizes([10000] * sp.count())
        else:
            sizes = sp.sizes()
            nsp = QSplitter(orientation)
            sp.insertWidget(idx, nsp)
            for x in ((new, src) if before else (src, new)):
                nsp.addWidget(x)
            nsp.setSizes([10000, 10000])
            sp.setSizes(sizes)
            src.show()
        self.set_active(new)
        if start:
            self.add_tab(StartTab(self), new)
        return new

    def remove_pane(self, pane):
        if len(self.panes) <= 1 or pane not in self.panes:
            return
        self.panes.remove(pane)
        sp = pane.parentWidget()
        pane.setParent(None)
        pane.deleteLater()
        while sp is not self.area and sp.count() == 0:
            parent = sp.parentWidget()
            sp.setParent(None)
            sp.deleteLater()
            sp = parent
        if self.active is pane or self.active not in self.panes:
            self.set_active(self.panes[-1])
        else:
            self.set_active(self.active)
        w = self.active.currentWidget()
        if w:
            w.setFocus()

    # ------------------------------------------------- session (disposition)
    def layout_state(self, w=None):
        """Décrit les volets, leurs onglets et leurs tailles pour la prochaine ouverture."""
        w = w or self.area
        if isinstance(w, Pane):
            tabs = []
            for i in range(w.count()):
                t = w.widget(i)
                if isinstance(t, Editor):
                    if t.path and not t.remote:
                        line, col = t.getCursorPosition()
                        tabs.append({"k": "file", "path": t.path, "line": line, "col": col,
                                     "cur": i == w.currentIndex()})
                elif isinstance(t, WebTab):
                    if t.path:
                        tabs.append({"k": "file", "path": t.path, "cur": i == w.currentIndex()})
                    elif t.view.url().scheme() in ("http", "https"):
                        tabs.append({"k": "web", "url": t.view.url().toString(),
                                     "cur": i == w.currentIndex()})
                elif isinstance(t, (ImageTab, PdfTab)):
                    tabs.append({"k": "file", "path": t.path, "cur": i == w.currentIndex()})
            return {"t": "pane", "tabs": tabs, "active": w is self.active}
        kids = [self.layout_state(w.widget(i)) for i in range(w.count())]
        return {"t": "split", "o": "h" if w.orientation() == Qt.Orientation.Horizontal else "v",
                "sizes": w.sizes(), "kids": kids}

    def restore_layout(self):
        try:
            state = json.loads(self.settings.value("layout", "") or "null")
        except ValueError:
            state = None
        if not isinstance(state, dict):
            return False
        spare = [self.panes[0]]  # le volet créé au démarrage sert pour le premier
        focus = []

        def build(node, parent):
            if node.get("t") == "split":
                kids = [k for k in node.get("kids", []) if isinstance(k, dict)]
                if not kids:
                    return
                o = Qt.Orientation.Horizontal if node.get("o") == "h" else Qt.Orientation.Vertical
                if parent is None:
                    sp = self.area
                    sp.setOrientation(o)
                else:
                    sp = QSplitter(o)
                    parent.addWidget(sp)
                for k in kids:
                    build(k, sp)
                sizes = node.get("sizes")
                if isinstance(sizes, list) and len(sizes) == sp.count() and sum(sizes) > 0:
                    sp.setSizes(sizes)
                return
            pane = spare.pop() if spare else self.new_pane()
            (parent or self.area).addWidget(pane)
            current = None
            for t in node.get("tabs", []):
                w = None
                if t.get("k") == "file" and os.path.isfile(t.get("path", "")):
                    w = self.open_file(t["path"], pane=pane)
                    if w is not None and "line" in t:
                        w.setCursorPosition(t["line"], t.get("col", 0))
                    w = w or next((x for x in self.all_tabs() if getattr(x, "path", None) == t["path"]), None)
                elif t.get("k") == "web" and HAS_WEB:
                    w = self.open_url(t.get("url"), pane=pane)
                if t.get("cur") and w is not None:
                    current = w
            if pane.count() == 0:
                self.add_tab(StartTab(self), pane)
            elif current is not None and pane.indexOf(current) >= 0:
                pane.setCurrentWidget(current)
            if node.get("active"):
                focus.append(pane)
        try:
            build(state, None)
        except Exception:
            return self.tabs.count() > 0
        target = focus[0] if focus else self.panes[0]
        self.set_active(target)
        if target.currentWidget():
            target.currentWidget().setFocus()
        return any(not isinstance(w, StartTab) for w in self.all_tabs())

    def close_pane(self):
        pane = self.active
        if len(self.panes) <= 1:
            return
        for i in reversed(range(pane.count())):
            w = pane.widget(i)
            if isinstance(w, Editor) and not self.confirm_close(w):
                return
            pane.removeTab(i)
            w.deleteLater()
        self.remove_pane(pane)

    def next_pane(self, d=1):
        if len(self.panes) > 1:
            p = self.panes[(self.panes.index(self.active) + d) % len(self.panes)]
            self.set_active(p)
            if p.currentWidget():
                p.currentWidget().setFocus()

    def move_tab(self, w, target):
        src = self.pane_of(w)
        if not src or target is src:
            return
        i = src.indexOf(w)
        ic, text, tip = src.tabIcon(i), src.tabText(i), src.tabToolTip(i)
        src.removeTab(i)
        self.add_tab(w, target, ic, text)
        target.setTabToolTip(target.indexOf(w), tip)
        if src.count() == 0:
            self.remove_pane(src)
            self.set_active(target)

    def move_tab_next(self):
        w = self.tabs.currentWidget()
        if w and len(self.panes) > 1:
            self.move_tab(w, self.panes[(self.panes.index(self.active) + 1) % len(self.panes)])

    def move_to_split(self, w, orientation):
        src = self.pane_of(w)
        if src and src.count() > 1:
            self.set_active(src)
            self.move_tab(w, self.split(orientation, start=False))

    def tab_menu(self, pane, pos):
        i = pane.tabBar().tabAt(pos)
        if i < 0:
            return
        w = pane.widget(i)
        m = QMenu(self)
        a_close = m.addAction(icon("close", COMMENT), "Fermer")
        m.addSeparator()
        a_r = m.addAction(icon("split_r", BLUE), "Déplacer dans un nouveau volet à droite")
        a_d = m.addAction(icon("split_d", BLUE), "Déplacer dans un nouveau volet en bas")
        a_n = m.addAction("Déplacer vers le volet suivant")
        for a in (a_r, a_d):
            a.setEnabled(pane.count() > 1)
        a_n.setEnabled(len(self.panes) > 1)
        r = m.exec(pane.tabBar().mapToGlobal(pos))
        if r == a_close:
            self.close_widget(w)
        elif r == a_r:
            self.move_to_split(w, Qt.Orientation.Horizontal)
        elif r == a_d:
            self.move_to_split(w, Qt.Orientation.Vertical)
        elif r == a_n:
            self.set_active(pane)
            pane.setCurrentWidget(w)
            self.move_tab_next()

    # ------------------------------------------------------------ web / PDF
    def favs(self):
        try:
            v = json.loads(self.settings.value("favoris", "") or "null")
        except ValueError:
            v = None
        return v if isinstance(v, list) else [list(f) for f in DEFAULT_FAVS]

    def set_favs(self, favs):
        self.settings.setValue("favoris", json.dumps(favs))

    def web_profile(self):
        if self._profile is None:
            load_web()
            self._profile = QWebEngineProfile("studentcode", self)  # cookies conservés
            self._profile.setHttpCacheMaximumSize(80 * 1024 * 1024)   # cache disque borné
            # garde aussi les cookies « de session » : on reste connecté aux sites d'un lancement à l'autre
            self._profile.setPersistentCookiesPolicy(
                QWebEngineProfile.PersistentCookiesPolicy.ForcePersistentCookies)
            self._profile.downloadRequested.connect(self.on_download)
            # identifiant de navigateur ordinaire : certains sites (connexion Google notamment)
            # refusent les navigateurs intégrés qui se déclarent « QtWebEngine »
            ua = re.sub(r"\s*QtWebEngine/\S+", "", self._profile.httpUserAgent())
            self._profile.setHttpUserAgent(ua)
        return self._profile

    def on_download(self, item):
        d = QStandardPaths.writableLocation(QStandardPaths.StandardLocation.DownloadLocation)
        item.setDownloadDirectory(d)
        item.accept()
        name = item.downloadFileName()
        self.statusBar().showMessage(f"Téléchargement de {name}…", 4000)

        def done():
            if item.isFinished():
                path = os.path.join(item.downloadDirectory(), item.downloadFileName())
                self.statusBar().showMessage(f"Téléchargé : {path}", 6000)
                if path.lower().endswith(".pdf") and os.path.isfile(path):
                    self.open_file(path)
        item.isFinishedChanged.connect(done)

    def open_url(self, url=None, pane=None):
        """Ouvre une page web dans un onglet (ou dans le navigateur si le moteur manque)."""
        if not HAS_WEB:
            if url:
                QDesktopServices.openUrl(QUrl(url if "://" in url else "https://" + url))
            self.statusBar().showMessage(
                "Module web absent : installe « PyQt6-WebEngine » depuis Bibliothèques.", 8000)
            return None
        tab = WebTab(self, url)
        self.add_tab(tab, pane)
        if not url:
            tab.url_in.setFocus()
        return tab

    # Boutons qui n'apparaissent que lorsqu'ils servent : pas à pas pendant le débogage, arrêt
    # pendant une exécution, débogueur / point d'arrêt / bibliothèques pour un fichier Python.
    CONTEXT_BUTTONS = ("debug", "over", "into", "outof", "pause", "stop", "bp", "libs")

    def toolbar_proxy(self, action):
        """Copie d'une action pour la barre d'outils : on peut la masquer sans couper le
        raccourci clavier ni l'entrée de menu de l'action d'origine."""
        proxy = QAction(self)

        def sync():
            proxy.setIcon(action.icon())
            proxy.setText(action.text())
            proxy.setToolTip(action.toolTip())
            proxy.setEnabled(action.isEnabled())
            proxy.setCheckable(action.isCheckable())
            proxy.setChecked(action.isChecked())
        sync()
        action.changed.connect(sync)
        proxy.triggered.connect(lambda *_: action.trigger())
        return proxy

    def update_toolbar(self):
        if not getattr(self, "tb_proxies", None):
            return
        ed = self.cur()
        python = ed is None or ed.is_python()
        debugging = getattr(self, "state", "idle") in ("debug_running", "debug_stopped")
        running = getattr(self, "state", "idle") != "idle"
        shown = {"debug": python, "bp": python, "libs": python, "stop": running,
                 "over": debugging, "into": debugging, "outof": debugging, "pause": debugging}
        key = tuple(sorted(shown.items()))
        if key == self._tb_shown:
            return
        self._tb_shown = key
        for n, vis in shown.items():
            self.tb_proxies[n].setVisible(vis)

    def set_state(self, state):
        self.state = state
        idle, stopped = state in ("idle", "post"), state == "debug_stopped"
        A = self.a
        A["run"].setEnabled(idle)
        A["debug"].setEnabled(idle or stopped)
        A["debug"].setText("Continuer" if stopped else "Déboguer")
        A["debug"].setIcon(icon("continue", GREEN) if stopped else icon("debug", ORANGE))
        for n in ("over", "into", "outof"):
            A[n].setEnabled(idle or stopped if n != "outof" else stopped)
        A["pause"].setEnabled(state == "debug_running")
        A["stop"].setEnabled(state != "idle")
        self.out.inp.setPlaceholderText(REPL_HINT if state == "post" else STDIN_HINT)
        self.eval_in.setEnabled(stopped)
        text, col = {"idle": ("", COMMENT), "post": ("  ⌨ interpréteur actif  ", BLUE), "running": ("  ▶ en cours  ", GREEN),
                     "debug_running": ("  ⏵ débogage : en cours  ", ORANGE),
                     "debug_stopped": ("  ⏸ débogage : à l'arrêt  ", YELLOW)}[state]
        self.state_lbl.setText(text)
        self.state_lbl.setStyleSheet(f"color: {col}; font-weight: 600;")
        self.update_toolbar()

    def update_status(self):
        if not hasattr(self, "mode_lbl"):
            return
        self.update_toolbar()
        ed = self.cur()
        mode = ed.mode if (ed and self.vim_on) else "ÉDITION"
        col = {"NORMAL": BLUE, "INSERT": GREEN, "VISUAL": MAGENTA, "V-LINE": MAGENTA}.get(mode, BLUE)
        self.mode_lbl.setText(f"  {mode}  ")
        self.mode_lbl.setStyleSheet(f"background: {col}; color: {BG_DARK}; font-weight: 700; padding: 3px 2px;")
        shown = self.tabs.currentWidget() if self.active else None
        if shown is not None and not isinstance(shown, Editor):
            where = shown.path or (shown.view.url().toString() if isinstance(shown, WebTab) else "")
            self.file_lbl.setText("  " + where)
            self.pos_lbl.setText("")
            self.setWindowTitle(f"{shown.title()} — StudentCode")
            if hasattr(self, "title_lbl"):
                self.title_lbl.setText(f"{shown.title()} — StudentCode")
        elif ed:
            line, col_ = ed.getCursorPosition()
            self.file_lbl.setText("  " + (("SSH " + ed.remote_name + " : " + ed.remote) if ed.remote else (ed.path or "sans titre")))
            self.pos_lbl.setText(f"Ln {line + 1}, Col {col_ + 1}   ")
            name = os.path.basename(ed.path) if ed.path else "sans titre"
            self.setWindowTitle(f"{name} — StudentCode")
            if hasattr(self, "title_lbl"):
                self.title_lbl.setText(f"{name} — StudentCode")
        else:
            self.file_lbl.setText("")
            self.pos_lbl.setText("")

    # ------------------------------------------------------------ fichiers
    def set_root(self, path):
        self.root = path
        self.tree.setRootIndex(self.fs.setRootPath(path))
        self.root_lbl.setText("  " + (os.path.basename(path) or path).upper())
        self.root_lbl.setToolTip(path)
        self.settings.setValue("root", path)  # retenu tout de suite, pas seulement à la fermeture
        if hasattr(self, "terminal"):
            self.terminal.cd(path)   # le terminal suit le dossier ouvert

    # ---------------------------------------- explorateur : dossiers dépliés retenus
    def tree_expanded(self, index, on):
        path = self.fs.filePath(index)
        if not path or self._restoring_tree:
            return
        if on:
            self.open_dirs.add(path)
        else:  # replier un dossier oublie aussi ses sous-dossiers
            self.open_dirs = {d for d in self.open_dirs if d != path and not d.startswith(path + os.sep)}
        self.settings.setValue("open_dirs", sorted(self.open_dirs))

    def restore_tree(self):
        """Redéplie les dossiers ouverts la dernière fois et remet le fichier courant en vue."""
        saved = self.settings.value("open_dirs", []) or []
        saved = [saved] if isinstance(saved, str) else list(saved)
        self.open_dirs = {d for d in saved if os.path.isdir(d) and
                          (d == self.root or d.startswith(self.root.rstrip(os.sep) + os.sep))}
        self._restoring_tree = True
        for d in sorted(self.open_dirs, key=len):  # les parents d'abord
            idx = self.fs.index(d)
            if idx.isValid():
                self.tree.expand(idx)
        self._restoring_tree = False
        ed = self.cur()
        if ed and ed.path and not ed.remote:
            idx = self.fs.index(ed.path)
            if idx.isValid():
                self.tree.setCurrentIndex(idx)
                QTimer.singleShot(400, lambda: self.tree.scrollTo(self.fs.index(ed.path))
                                  if ed in self.editors() else None)

    def add_editor(self, ed, title, pane=None):
        self.add_tab(ed, pane, file_icon(ed.path or "x.py"), title)
        ed.modificationChanged.connect(lambda _: self.retitle(ed))
        self.last_editor = ed
        return ed

    def retitle(self, ed):
        pane = self.pane_of(ed)
        if pane:
            i = pane.indexOf(ed)
            name = os.path.basename(ed.path) if ed.path else ed.property("title")
            if ed.remote:
                name = (ed.remote_name + " ⇄ " if ed.remote_name else "⇄ ") + name
            pane.setTabText(i, name + ("  ●" if ed.isModified() or ed.unsent else ""))
            pane.setTabToolTip(i, ("SSH " + ed.remote_name + " : " + ed.remote) if ed.remote else (ed.path or ""))
            pane.setTabIcon(i, file_icon(ed.path or "x.py"))

    def new_file(self, text=""):
        self.untitled += 1
        ed = Editor(self)
        title = f"sans_titre_{self.untitled}.py"
        ed.setProperty("title", title)
        ed.setText(text or "")
        ed.setModified(False)
        return self.add_editor(ed, title)

    def open_file(self, path, line=None, pane=None):
        path = os.path.abspath(path)
        ext = os.path.splitext(path)[1].lower()
        w = next((t for t in self.all_tabs() if getattr(t, "path", None) == path), None)
        if w is not None:
            p = self.pane_of(w)
            p.setCurrentWidget(w)
            self.set_active(p)
        elif ext == ".pdf" and HAS_PDF:
            try:
                w = self.add_tab(PdfTab(self, path), pane)
            except Exception as e:
                self.statusBar().showMessage(f"PDF illisible : {e}", 8000)
                return None
        elif ext == ".pdf":
            if not HAS_WEB:
                QDesktopServices.openUrl(QUrl.fromLocalFile(path))
                return None
            w = self.add_tab(WebTab(self, path=path), pane)
        elif ext in IMG_EXT:
            w = self.add_tab(ImageTab(self, path), pane)
        else:
            try:
                with open(path, encoding="utf-8") as fh:
                    text = fh.read()
            except (OSError, UnicodeDecodeError) as e:
                self.statusBar().showMessage(f"Impossible d'ouvrir {path} : {e}", 6000)
                return None
            # un saut du débogueur ouvre le code à côté du code, pas dans le volet web
            if pane is None and line and self.last_editor in self.editors():
                pane = self.pane_of(self.last_editor)
            target = pane if pane in self.panes else self.active
            first = target.currentWidget()
            if target.count() == 1 and isinstance(first, Editor) and not first.path \
                    and not first.isModified() and not first.text().strip():
                target.removeTab(0)
                first.deleteLater()
            w = Editor(self, path)
            w.setText(text)
            w.setModified(False)
            self.add_editor(w, os.path.basename(path), target)
            self.retitle(w)
        if not isinstance(w, Editor):
            self.update_status()
            return None
        if line:
            w.setCursorPosition(line - 1, 0)
            w.ensureLineVisible(line - 1)
        w.setFocus()
        self.last_editor = w
        self.update_status()
        return w

    def open_dialog(self):
        paths, _ = QFileDialog.getOpenFileNames(self, "Ouvrir", self.root,
                                                "Tous les fichiers (*);;Python (*.py *.pyw);;PDF et images (*.pdf *.png *.jpg *.jpeg *.gif)")
        for p in paths:
            self.open_file(p)

    def open_folder(self):
        d = QFileDialog.getExistingDirectory(self, "Ouvrir un dossier", self.root)
        if d:
            self.set_root(d)
            self.left.show()

    def save(self, ed=None, then=None):
        ed = ed or self.cur()
        if not ed:
            return False
        if not ed.path:
            return self.save_as(ed)
        try:
            with open(ed.path, "w", encoding="utf-8", newline="\n") as fh:
                fh.write(ed.text())
        except OSError as e:
            QMessageBox.critical(self, "Erreur", f"Enregistrement impossible :\n{e}")
            return False
        ed.setModified(False)
        if ed.remote:  # fichier d'un lab : on l'envoie sur la machine
            self.ssh_hub.upload(ed, then)
            return True
        self.statusBar().showMessage(f"Enregistré : {ed.path}", 2500)
        if then:
            then()
        return True

    def save_as(self, ed=None):
        ed = ed or self.cur()
        if not ed:
            return False
        start = ed.path or os.path.join(self.root, ed.property("title") or "script.py")
        path, _ = QFileDialog.getSaveFileName(self, "Enregistrer sous", start,
                                              "Python (*.py);;Tous les fichiers (*)")
        if not path:
            return False
        ed.path = os.path.abspath(path)
        ed.apply_lexer()
        ok = self.save(ed)
        self.retitle(ed)
        self.update_status()
        return ok

    def confirm_close(self, ed):
        if ed.unsent and not ed.isModified():
            return QMessageBox.question(
                self, "Fichier pas encore envoyé",
                f"« {os.path.basename(ed.path)} » n'a pas pu être envoyé sur la machine distante.\n"
                "Fermer quand même ? (Une copie reste dans le cache local.)") == QMessageBox.StandardButton.Yes
        if not ed.isModified():
            return True
        name = os.path.basename(ed.path) if ed.path else ed.property("title")
        B = QMessageBox.StandardButton
        r = QMessageBox.question(self, "Modifications non enregistrées",
                                 f"Enregistrer les modifications de « {name} » ?",
                                 B.Save | B.Discard | B.Cancel, B.Save)
        if r == B.Save:
            return self.save(ed)
        return r == B.Discard

    def close_tab(self, i, force=False):
        self.close_widget(self.tabs.widget(i), force)

    def close_widget(self, w, force=False):
        pane = self.pane_of(w) if w else None
        if not pane:
            return
        if isinstance(w, Editor) and not force and not self.confirm_close(w):
            return
        pane.removeTab(pane.indexOf(w))
        if isinstance(w, PdfTab):
            w.close_doc()
        w.deleteLater()
        if pane.count() == 0 and len(self.panes) > 1:
            self.remove_pane(pane)
        self.update_status()

    def next_tab(self, d):
        n = self.tabs.count()
        if n:
            self.tabs.setCurrentIndex((self.tabs.currentIndex() + d) % n)
            self.tabs.currentWidget().setFocus()

    def tree_open(self, index):
        path = self.fs.filePath(index)
        if os.path.isfile(path):
            self.open_file(path)

    def tree_menu(self, pos):
        index = self.tree.indexAt(pos)
        if index.isValid() and not self.tree.selectionModel().isSelected(index):
            self.tree.setCurrentIndex(index)
        sel = self.tree.selected_paths() if index.isValid() else []
        path = self.fs.filePath(index) if index.isValid() else self.root
        base = path if os.path.isdir(path) else os.path.dirname(path)
        m = QMenu(self)
        a_new = m.addAction(icon("new", FG), "Nouveau fichier…")
        a_dir = m.addAction(icon("folder", BLUE), "Nouveau dossier…")
        a_ren = a_mv = a_del = a_side = None
        if sel and os.path.isfile(path):
            m.addSeparator()
            a_side = m.addAction(icon("split_r", BLUE), "Ouvrir dans un volet à côté")
        if sel:
            m.addSeparator()
            n = f" ({len(sel)} éléments)" if len(sel) > 1 else ""
            if len(sel) == 1:
                a_ren = m.addAction("Renommer…  (F2)")
            a_mv = m.addAction(icon("move", YELLOW), f"Déplacer vers…{n}")
            a_del = m.addAction(icon("trash", RED), f"Mettre à la corbeille{n}  (Suppr)")
        m.addSeparator()
        a_here = None
        if index.isValid() and os.path.isdir(path):
            a_here = m.addAction(icon("folder", BLUE), "Ouvrir ce dossier comme racine")
        a_root = m.addAction(icon("open", YELLOW), "Ouvrir un autre dossier…")
        r = m.exec(self.tree.viewport().mapToGlobal(pos))
        if r is None:
            return
        if r == a_root:
            self.open_folder()
        elif r == a_here:
            self.set_root(path)
        elif r == a_side:
            self.open_file(path, pane=self.split(Qt.Orientation.Horizontal, start=False))
        elif r in (a_new, a_dir):
            name, ok = QInputDialog.getText(self, r.text(), "Nom :",
                                            text="script.py" if r == a_new else "dossier")
            if not (ok and name.strip()):
                return
            target = os.path.join(base, name.strip())
            if os.path.exists(target):
                QMessageBox.warning(self, "Existe déjà", f"« {name} » existe déjà.")
                return
            try:
                if r == a_new:
                    open(target, "w", encoding="utf-8").close()
                    self.open_file(target)
                else:
                    os.makedirs(target)
            except OSError as e:
                QMessageBox.critical(self, "Erreur", str(e))
        elif r == a_ren:
            self.rename_path(sel[0])
        elif r == a_mv:
            dest = QFileDialog.getExistingDirectory(self, "Déplacer vers", base)
            if dest:
                self.move_paths(sel, dest)
        elif r == a_del:
            self.trash_paths(sel)

    def repath(self, old, new):
        """Met à jour les onglets ouverts après un déplacement / renommage."""
        for ed in self.editors():
            if ed.path and (ed.path == old or ed.path.startswith(old + os.sep)):
                ed.path = new + ed.path[len(old):]
                ed.apply_lexer()
                self.retitle(ed)
        self.update_status()

    def rename_path(self, path):
        name, ok = QInputDialog.getText(self, "Renommer", "Nouveau nom :",
                                        text=os.path.basename(path))
        if not (ok and name.strip()) or name == os.path.basename(path):
            return
        new = os.path.join(os.path.dirname(path), name.strip())
        if os.path.exists(new):
            QMessageBox.warning(self, "Existe déjà", f"« {name} » existe déjà.")
            return
        try:
            os.rename(path, new)
        except OSError as e:
            QMessageBox.critical(self, "Erreur", str(e))
            return
        self.repath(path, new)

    def move_paths(self, paths, dest, copy=False):
        done, errors = 0, []
        for p in paths:
            p = os.path.abspath(p)
            name = os.path.basename(p)
            target = os.path.join(dest, name)
            if os.path.dirname(p) == dest:
                continue
            if dest == p or dest.startswith(p + os.sep):
                errors.append(f"« {name} » : impossible de déplacer un dossier dans lui-même")
                continue
            if os.path.exists(target):
                errors.append(f"« {name} » existe déjà dans la destination")
                continue
            try:
                if copy:
                    shutil.copytree(p, target) if os.path.isdir(p) else shutil.copy2(p, target)
                else:
                    shutil.move(p, target)
                    self.repath(p, target)
                done += 1
            except OSError as e:
                errors.append(f"« {name} » : {e}")
        if done:
            verb = "copié(s)" if copy else "déplacé(s)"
            self.statusBar().showMessage(
                f"{done} élément(s) {verb} vers {os.path.basename(dest) or dest}", 4000)
        if errors:
            QMessageBox.warning(self, "Déplacement", "\n".join(errors))

    def trash_paths(self, paths):
        if not paths:
            return
        what = f"« {os.path.basename(paths[0])} »" if len(paths) == 1 else f"ces {len(paths)} éléments"
        if QMessageBox.question(self, "Corbeille", f"Mettre {what} à la corbeille ?") \
                != QMessageBox.StandardButton.Yes:
            return
        failed = [os.path.basename(p) for p in paths if not QFile.moveToTrash(p)]
        if failed:
            QMessageBox.warning(self, "Erreur", "Impossible de mettre à la corbeille :\n"
                                + "\n".join(failed))

    # ------------------------------------------------- recherche / commandes
    def show_finder(self):
        Finder(self).exec()

    def show_find(self):
        ed = self.cur()
        if ed and ed.hasSelectedText() and "\n" not in ed.selectedText():
            self.find_in.setText(ed.selectedText())
        self.findbar.show()
        self.find_in.setFocus()
        self.find_in.selectAll()

    def hide_find(self):
        self.findbar.hide()
        if self.cur():
            self.cur().setFocus()

    def find_next(self, forward=True):
        ed, text = self.cur(), self.find_in.text()
        if not (ed and text):
            return False
        if forward:
            ok = ed.findFirst(text, False, False, False, True, True)
        else:
            l1, i1, _, _ = ed.getSelection() if ed.hasSelectedText() else (*ed.getCursorPosition(), 0, 0)
            ok = ed.findFirst(text, False, False, False, True, False, l1, i1)
        if not ok:
            self.statusBar().showMessage(f"« {text} » introuvable", 2500)
        return ok

    def replace_one(self):
        ed = self.cur()
        if ed and ed.hasSelectedText() and ed.selectedText().lower() == self.find_in.text().lower():
            ed.replace(self.repl_in.text())
        self.find_next(True)

    def replace_all(self):
        ed, text = self.cur(), self.find_in.text()
        if not (ed and text):
            return
        n = 0
        ed.beginUndoAction()
        ok = ed.findFirst(text, False, False, False, False, True, 0, 0)
        while ok:
            ed.replace(self.repl_in.text())
            n += 1
            ok = ed.findNext()
        ed.endUndoAction()
        self.statusBar().showMessage(f"{n} remplacement(s)", 3000)

    def goto_dialog(self):
        ed = self.cur()
        if ed:
            n, ok = QInputDialog.getInt(self, "Aller à la ligne", "Ligne :",
                                        ed.getCursorPosition()[0] + 1, 1, ed.lines())
            if ok:
                ed.setCursorPosition(n - 1, 0)
                ed.setFocus()

    def show_cmdline(self):
        self.cmdline.clear()
        self.cmdline.show()
        self.cmdline.setFocus()

    def hide_cmdline(self):
        self.cmdline.hide()
        if self.cur():
            self.cur().setFocus()

    def vim_command(self, cmd):
        cmd = cmd.strip().lstrip(":")
        self.hide_cmdline()
        ed = self.cur()
        i = self.tabs.currentIndex()
        if cmd.isdigit() and ed:
            ed.setCursorPosition(int(cmd) - 1, 0)
        elif cmd == "w":
            self.save()
        elif cmd in ("wq", "x"):
            if self.save():
                self.close_tab(i)
        elif cmd in ("q", "bd"):
            self.close_tab(i)
        elif cmd in ("q!", "bd!"):
            self.close_tab(i, force=True)
        elif cmd in ("qa", "qa!"):
            self.close()
        elif cmd.startswith("e "):
            p = os.path.expanduser(cmd[2:].strip())
            p = p if os.path.isabs(p) else os.path.join(self.root, p)
            if not os.path.exists(p):
                open(p, "w").close()
            self.open_file(p)
        elif cmd:
            self.statusBar().showMessage(f"Commande inconnue : {cmd}", 3000)

    def show_ssh(self):
        self.left.show()
        self.left.setCurrentWidget(self.ssh_hub)
        if self.ssh_panel.lab is None:
            self.ssh_panel.connect()

    # ------------------------------------------------------------------ IA
    def toggle_ai(self):
        show = not self.ai_panel.isVisible()
        self.ai_panel.setVisible(show)
        self.ai_btn.setChecked(show)
        if show:
            sizes = self.hsplit.sizes()
            if sizes[-1] < 300:  # lui donner une largeur utile, prise sur la zone centrale
                sizes[1] = max(sizes[1] - (460 - sizes[-1]), 300)
                sizes[-1] = 460
                self.hsplit.setSizes(sizes)
            self.ai_panel.refresh_context_label()
            self.ai_panel.input.setFocus()

    def ai_preprompt(self):
        return self.settings.value("ai_preprompt", "") or AI_PREPROMPT

    def edit_preprompt(self):
        text, ok = QInputDialog.getMultiLineText(
            self, "Pré-prompt de l'IA",
            "Consigne envoyée à l'IA avant ton code (laisse vide pour revenir au texte par défaut) :",
            self.ai_preprompt())
        if ok:
            self.settings.setValue("ai_preprompt", "" if text.strip() == AI_PREPROMPT else text.strip())

    def ai_context(self):
        """(libellé, bloc de texte) décrivant le fichier courant ou la sélection."""
        ed = self.cur()
        if ed is None:
            return "", ""
        name = os.path.basename(ed.remote or ed.path or ed.property("title") or "fichier")
        lang = LANGS.get(os.path.splitext(name)[1].lower(), "")
        if ed.hasSelectedText():
            l1, _, l2, _ = ed.getSelection()
            code = ed.selectedText()
            label = f"sélection de {name} (lignes {l1 + 1} à {l2 + 1})"
            what = f"Extrait sélectionné de {name} (lignes {l1 + 1} à {l2 + 1})"
        else:
            code = ed.text()
            label = name
            what = f"Fichier {name} ({ed.lines()} lignes ; mon curseur est ligne {ed.getCursorPosition()[0] + 1})"
        if len(code) > 60000:
            code = code[:60000] + "\n# … (fichier tronqué)"
        return label, what + " :\n```" + lang + "\n" + code.rstrip("\n") + "\n```"

    def ai_output(self, force=False):
        """Dernières lignes de la sortie du programme, si elles aident (erreur) ou si on les demande."""
        out = self.out.out.toPlainText().strip()
        if not out or not (force or "Traceback" in out or "Error" in out):
            return ""
        return "Sortie du programme (dernières lignes) :\n```\n" + "\n".join(out.splitlines()[-60:]) + "\n```"

    def open_ai_site(self, site, prompt):
        """Connexion « site web » : copie le message, ouvre l'assistant à côté du code et le colle."""
        QApplication.clipboard().setText(prompt)
        url = AI_SITES.get(site) or AI_SITES["ChatGPT"]
        hint = f"Message copié pour {site} — s'il n'apparaît pas dans la zone de saisie, clique dedans puis Ctrl+V."
        if not HAS_WEB:
            QDesktopServices.openUrl(QUrl(url))
            self.statusBar().showMessage(hint, 12000)
            return
        tab = next((t for t in self.all_tabs() if isinstance(t, WebTab) and getattr(t, "ai_site", None)), None)

        def paste():
            if tab in self.all_tabs():
                tab.view.setFocus()
                load_web()
                tab.view.page().triggerAction(QWebEnginePage.WebAction.Paste)
        if tab is not None and tab.ai_site == site:
            self.pane_of(tab).setCurrentWidget(tab)
            QTimer.singleShot(150, paste)  # même discussion : on colle directement
        else:
            if tab is None:
                code_pane = self.pane_of(self.cur()) if self.cur() else self.active
                others = [p for p in self.panes if p is not code_pane]
                if others:
                    pane = others[-1]
                else:
                    self.set_active(code_pane)
                    pane = self.split(Qt.Orientation.Horizontal, start=False)
                tab = self.open_url(url, pane=pane)
            else:
                self.pane_of(tab).setCurrentWidget(tab)
                tab.load(url)
            tab.ai_site = site

            def loaded(ok, t=tab):
                try:
                    t.view.loadFinished.disconnect(loaded)
                except TypeError:
                    pass
                QTimer.singleShot(1800, paste)  # le temps que la zone de saisie soit prête
            tab.view.loadFinished.connect(loaded)
        self.statusBar().showMessage(hint, 12000)

    def choose_theme(self, name):
        """Enregistre le thème puis relance l'application (la session est restaurée)."""
        for a in self.theme_menu.actions():
            a.setChecked(a.text() == theme.NAME)
        if name == theme.NAME:
            return
        theme.set_theme(name)
        if not self.close():  # l'utilisateur a annulé (fichier non enregistré)
            theme.set_theme(theme.NAME)
            return
        if not os.environ.get("STUDENTCODE_NO_RESTART"):
            QProcess.startDetached(*restart_cmd())

    def bottom_menu(self, pos):
        bar = self.bottom.tabBar()
        i = bar.tabAt(pos)
        w = self.bottom.widget(i) if i >= 0 else None
        hub = self.ssh_hub
        panel = hub.panel_of(w) if w is not None else None
        m = QMenu(self)
        acts = {}
        if panel is not None and panel.lab is not None:
            acts[m.addAction(icon("terminal", GREEN), "Nouveau terminal (même machine)")] = lambda: panel.new_terminal()
            if w in panel.extras:
                acts[m.addAction(icon("close", RED), "Fermer ce terminal")] = lambda: panel.close_terminal(w)
        others = [p for p in hub.panels() if getattr(p, "lab", None) is not None and p is not panel]
        if others:
            if acts:
                m.addSeparator()
            for p in others:
                acts[m.addAction(icon("server", GREEN), "Nouveau terminal SSH " + hub.title(p))] = (lambda p=p: p.new_terminal())
        if not acts:
            m.addAction("Connecte-toi à une machine (onglet SSH à gauche) pour ouvrir un terminal SSH").setEnabled(False)
        r = m.exec(bar.mapToGlobal(pos))
        if r in acts:
            acts[r]()

    def show_terminal(self):
        self.bottom.show()
        self.bottom.setCurrentWidget(self.terminal)
        self.terminal.setFocus()

    # ------------------------------------------------- barres de défilement
    def style_scrollbars(self, area):
        """Montre ou masque les barres d'une zone qui défile ; la molette fonctionne toujours."""
        pol = Qt.ScrollBarPolicy.ScrollBarAsNeeded if self.scrollbars else Qt.ScrollBarPolicy.ScrollBarAlwaysOff
        area.setVerticalScrollBarPolicy(pol)
        area.setHorizontalScrollBarPolicy(pol)

    def apply_scrollbars(self):
        from PyQt6.QtWidgets import QAbstractScrollArea
        from terminal import TermWidget
        TermWidget.show_bar = self.scrollbars
        for area in self.findChildren(QAbstractScrollArea):
            self.style_scrollbars(area)
        for t in self.findChildren(TermWidget):
            t.sync_bar()
            t.resize_timer.start()   # la place de la barre change le nombre de colonnes

    def toggle_webgpu(self):
        self.settings.setValue("web_gpu", self.a["webgpu"].isChecked())
        self.statusBar().showMessage("Pris en compte au prochain lancement de StudentCode.", 5000)

    def toggle_scrollbars(self):
        self.scrollbars = self.a["scrollbars"].isChecked()
        self.settings.setValue("scrollbars", self.scrollbars)
        self.apply_scrollbars()

    def toggle_explorer(self):
        self.left.setVisible(not self.left.isVisible())

    def toggle_vim(self):
        self.vim_on = self.a["vim"].isChecked()
        for ed in self.editors():
            ed.set_mode("NORMAL" if self.vim_on else "INSERT")
        self.update_status()

    def show_help(self):
        box = QMessageBox(self)
        box.setWindowTitle(f"StudentCode {__version__} — raccourcis clavier")
        box.setTextFormat(Qt.TextFormat.RichText)
        box.setText(SHORTCUTS)
        box.exec()

    # -------------------------------------------------------- interpréteur
    def start_repl(self):
        old = getattr(self, "repl_proc", None)
        if old:
            old.readyReadStandardOutput.disconnect()
            old.readyReadStandardError.disconnect()
            old.kill()
            old.deleteLater()
        self.repl.out.clear()
        p = self.repl_proc = QProcess(self)
        p.setProcessEnvironment(self.proc_env())
        p.setWorkingDirectory(self.root)
        p.readyReadStandardOutput.connect(lambda: self.repl.write(
            bytes(p.readAllStandardOutput()).decode("utf-8", "replace")))
        p.readyReadStandardError.connect(lambda: self.repl.write(self.clean_err(
            "repl", bytes(p.readAllStandardError()).decode("utf-8", "replace")), RED))
        boot = ("import code,sys;print('Python',sys.version.split()[0]);"
                "code.interact(banner='',exitmsg='',local={'__name__':'__main__'})")
        py = python_cmd()
        if py is None:
            self.repl.write(PYTHON_MISSING + "\n", YELLOW)
            return
        p.start(py[0], py[1:] + ["-u", "-c", boot])

    def repl_send(self, text):
        self.repl.write(text + "\n", CYAN)
        if self.repl_proc is None or self.repl_proc.state() != QProcess.ProcessState.Running:
            self.start_repl()
        self.repl_proc.write((text + "\n").encode("utf-8"))

    # ---------------------------------------------------------- exécution
    def clean_err(self, key, text):
        """Retire de la sortie d'erreur les avertissements de Qt/GTK (thème du bureau,
        Wayland) que les programmes graphiques affichent sans qu'il y ait de vrai problème."""
        buf = self._errbuf.get(key, "") + text
        lines = buf.split("\n")
        tail = lines.pop()
        # une ligne incomplète n'est retenue que si elle ressemble au début d'un avertissement
        hold = tail if tail[:1] in ("(", "q", "Q", "l") and len(tail) < 400 and \
            (NOISE.match(tail) or len(tail) < 12) else ""
        self._errbuf[key] = hold
        keep = [ln for ln in lines if not NOISE.match(ln)]
        out = "\n".join(keep) + ("\n" if keep else "")
        return out + ("" if hold else tail)

    def proc_env(self):
        env = QProcessEnvironment()
        for k, v in child_env().items():
            env.insert(k, v)
        rules = env.value("QT_LOGGING_RULES", "")
        env.insert("QT_LOGGING_RULES", (rules + ";" if rules else "") + QUIET_QT)
        env.insert("PYTHONUNBUFFERED", "1")
        env.insert("PYTHONIOENCODING", "utf-8")
        return env

    def ask_args(self):
        text, ok = QInputDialog.getText(self, "Arguments du programme",
                                        "Arguments passés au script (sys.argv) :",
                                        text=shlex.join(self.run_args))
        if ok:
            try:
                self.run_args = shlex.split(text)
            except ValueError as e:
                QMessageBox.warning(self, "Arguments invalides", str(e))

    def start(self, debug, stop_entry=False):
        ed = self.cur()
        if not ed or self.rrun:
            return
        if ed.remote:
            return self.ssh_hub.run_remote(ed, debug)
        if self.proc:
            if self.state != "post":
                return
            self.kill_proc()  # relance : on ferme l'interpréteur de l'exécution précédente
        for e in self.editors():
            if e.path and e.isModified():
                self.save(e)
        if not ed.path and not self.save_as(ed):
            return
        other = None
        if not ed.is_python():   # C, C++, Java, shell… : compilé puis lancé, sans débogueur Python
            if WINDOWS:
                other, why = windows_command(ed.path)
            else:
                other, why = posix_command(ed.path), "Je ne sais pas exécuter ce type de fichier."
            if other is None:
                self.statusBar().showMessage(why, 10000)
                return
        elif python_cmd() is None:   # app empaquetée sans Python installé sur l'ordinateur
            self.bottom.show()
            self.bottom.setCurrentWidget(self.out)
            self.out.write(PYTHON_MISSING + "\n", YELLOW)
            return
        if other is not None and debug:
            self.statusBar().showMessage(
                "Le débogueur pas à pas ne fonctionne que pour Python ; ici, utilise Exécuter (F5).", 7000)
            return
        self.clear_marks(errors=True)
        self.vars.clear()
        self.stack_list.clear()
        self.out.out.clear()
        self.bottom.setCurrentWidget(self.out)
        self.bottom.show()
        label = "Débogage" if debug else "Exécution"
        self.out.write(f"── {label} : {os.path.basename(ed.path)} ──\n", COMMENT)
        p = self.proc = QProcess(self)
        p.setProcessEnvironment(self.proc_env())
        p.setWorkingDirectory(os.path.dirname(ed.path))
        p.readyReadStandardOutput.connect(lambda: self.prog_out(
            bytes(p.readAllStandardOutput()).decode("utf-8", "replace")))
        p.readyReadStandardError.connect(lambda: self.out.write(self.clean_err(
            "run", bytes(p.readAllStandardError()).decode("utf-8", "replace")), RED))
        p.finished.connect(self.finished)
        p.errorOccurred.connect(self.proc_error)
        args = ["-u"]
        if debug:
            self.right.show()
            self.server = QTcpServer(self)
            self.server.listen(QHostAddress(QHostAddress.SpecialAddress.LocalHost), 0)
            self.server.newConnection.connect(self.dbg_connected)
            args += [BACKEND, str(self.server.serverPort()), "1" if stop_entry else "0"]
        else:
            args += [RUNNER]
        args += [ed.path] + self.run_args
        self.set_state("debug_running" if debug else "running")
        if other is not None:
            prog, sargs = shell_cmd(other + "".join(" " + quote(a) for a in self.run_args))
            if WINDOWS:   # cmd.exe a ses propres règles de guillemets : arguments passés tels quels
                p.setProgram(prog)
                p.setNativeArguments(" ".join(sargs))
                p.start()
            else:
                p.start(prog, sargs)
        else:
            py = python_cmd()
            p.start(py[0], py[1:] + args)

    def proc_error(self, err):
        if err == QProcess.ProcessError.FailedToStart and self.proc:
            self.out.write("Impossible de lancer l'interpréteur Python.\n", RED)
            self.finished(-1, None)

    def stop(self):
        if self.rrun:
            self.rrun.ssh.run_stop()
        elif self.proc:
            self.proc.kill()

    def kill_proc(self):
        p = self.proc
        if p:
            p.blockSignals(True)
            p.kill()
            p.waitForFinished(1500)
            self.finished(0, None, quiet=True)

    def prog_out(self, text):
        """Sortie du programme ; repère le signal de fin envoyé par le lanceur."""
        m = END_RE.search(text)
        if not m:
            self.out.write(text)
            return
        self.out.write(text[:m.start()])
        # laisse arriver les derniers messages (stderr, débogueur) avant le bandeau
        if self.proc:
            self.out.write(self.clean_err("run", bytes(self.proc.readAllStandardError()).decode("utf-8", "replace")), RED)
        if self.dsock:
            self.dsock.waitForReadyRead(80)
            self.dbg_read()
        code = int(m.group(1))
        self.out.write(f"\n── Programme terminé (code {code}) · interpréteur actif : "
                       "les variables, fonctions et classes du programme sont disponibles ──\n",
                       COMMENT if code == 0 else YELLOW)
        self.clear_marks()
        self.set_state("post")
        self.out.inp.setFocus()
        self.out.write(text[m.end():])

    def finished(self, code, status, quiet=False):
        p = self.proc
        if not p:
            return
        self.proc = None
        was_post = self.state == "post"
        self.out.write(bytes(p.readAllStandardOutput()).decode("utf-8", "replace"))
        self.out.write(self.clean_err("run", bytes(p.readAllStandardError()).decode("utf-8", "replace")), RED)
        if self.dsock:
            self.dsock.waitForReadyRead(150)  # dernier message (exception) du débogueur
            self.dbg_read()
            self.dsock.deleteLater()
            self.dsock = None
        if self.server:
            self.server.close()
            self.server.deleteLater()
            self.server = None
        crashed = status == QProcess.ExitStatus.CrashExit
        if was_post:
            if not quiet:
                self.out.write("\n── Interpréteur fermé ──\n", COMMENT)
        else:
            msg = "arrêté" if crashed else f"terminé (code {code})"
            self.out.write(f"\n── Programme {msg} ──\n", COMMENT if code == 0 and not crashed else YELLOW)
        p.deleteLater()
        self.clear_marks()
        self.set_state("idle")

    def send_stdin(self, text):
        if self.rrun:
            self.out.write(text + "\n", CYAN)
            self.rrun.ssh.run_send(text + "\n")
        elif self.proc:
            self.out.write(text + "\n", CYAN)
            self.proc.write((text + "\n").encode("utf-8"))
        else:
            self.statusBar().showMessage("Lance d'abord le programme (F5) : tu pourras ensuite taper du Python ici.", 5000)

    # ---------------------------------------------------------- débogueur
    def all_breaks(self):
        return [[e.path, ln] for e in self.editors() if e.path for ln in e.breakpoints()]

    def dbg_connected(self):
        self.dsock = self.server.nextPendingConnection()
        self.dbuf = b""
        self.dsock.readyRead.connect(self.dbg_read)
        self.dbg_send({"cmd": "init", "breaks": self.all_breaks()})

    def dbg_send(self, obj):
        if self.dsock:
            self.dsock.write((json.dumps(obj) + "\n").encode("utf-8"))
            self.dsock.flush()

    def dbg_read(self):
        if not self.dsock:
            return
        self.dbuf += bytes(self.dsock.readAll())
        while b"\n" in self.dbuf:
            line, self.dbuf = self.dbuf.split(b"\n", 1)
            try:
                self.dbg_event(json.loads(line))
            except ValueError:
                pass

    def dbg_event(self, m):
        ev = m.get("event")
        if ev == "stopped":
            self.stack = m["stack"]
            self.set_state("debug_stopped")
            self.stack_list.blockSignals(True)
            self.stack_list.clear()
            for f in self.stack:
                self.stack_list.addItem(f"{f['func']}   {os.path.basename(f['file'])}:{f['line']}")
            self.stack_list.setCurrentRow(0)
            self.stack_list.blockSignals(False)
            self.select_frame(0)
            self.activateWindow()
        elif ev == "vars":
            if m["frame"] == self.frame:
                self.fill_vars(m)
        elif ev == "children":
            item = self.child_req.pop(m["id"], None)
            if item is not None:
                item.takeChildren()
                for d in m["items"]:
                    self.add_var(item, d)
        elif ev == "eval":
            if m["text"]:
                self.out.write(m["text"] + "\n", RED if m.get("error") else TEAL)
            self.dbg_send({"cmd": "vars", "frame": self.frame})
        elif ev == "exception":
            if m.get("file"):
                ed = self.open_file(m["file"], m["line"])
                if ed:
                    ed.markerAdd(m["line"] - 1, ERRLINE)
            self.statusBar().showMessage(m["text"], 10000)

    def select_frame(self, i):
        if self.state != "debug_stopped" or not (0 <= i < len(self.stack)):
            return
        self.frame = i
        f = self.stack[i]
        self.clear_marks()
        ed = self.open_file(f["file"], f["line"])
        if ed:
            ed.markerAdd(f["line"] - 1, CURLINE)
            ed.markerAdd(f["line"] - 1, CUR)
        self.dbg_send({"cmd": "vars", "frame": i})

    def clear_marks(self, errors=False):
        for e in self.editors():
            e.markerDeleteAll(CUR)
            e.markerDeleteAll(CURLINE)
            if errors:
                e.markerDeleteAll(ERRLINE)

    def fill_vars(self, m):
        self.vars.clear()
        self.child_req.clear()
        for title, key in (("Locales", "locals"), ("Globales", "globals")):
            if not m[key]:
                continue
            top = QTreeWidgetItem([title])
            top.setForeground(0, QColor(COMMENT))
            self.vars.addTopLevelItem(top)
            for d in m[key]:
                self.add_var(top, d)
            top.setExpanded(key == "locals" or not m["locals"])

    def add_var(self, parent, d):
        it = QTreeWidgetItem([d["name"], d["type"], d["repr"]])
        it.setForeground(0, QColor(BLUE))
        it.setForeground(1, QColor(COMMENT))
        it.setForeground(2, QColor(GREEN if d["type"].startswith("str") else FG))
        it.setToolTip(2, d["repr"])
        it.setData(0, Qt.ItemDataRole.UserRole, d["expr"])
        if d["exp"]:
            it.addChild(QTreeWidgetItem(["…"]))
            it.setData(1, Qt.ItemDataRole.UserRole, True)
        parent.addChild(it)

    def var_expanded(self, it):
        if it.data(1, Qt.ItemDataRole.UserRole) and self.state == "debug_stopped":
            it.setData(1, Qt.ItemDataRole.UserRole, False)
            self.req_id += 1
            self.child_req[self.req_id] = it
            self.dbg_send({"cmd": "children", "frame": self.frame, "id": self.req_id,
                           "expr": it.data(0, Qt.ItemDataRole.UserRole)})

    def debug_eval(self, text):
        if text.strip() and self.state == "debug_stopped":
            self.bottom.setCurrentWidget(self.out)
            self.out.write(f"(debug) {text}\n", MAGENTA)
            self.dbg_send({"cmd": "eval", "frame": self.frame, "expr": text})

    def resume(self, cmd):
        self.clear_marks()
        self.set_state("debug_running")
        self.dbg_send({"cmd": cmd})

    def debug_or_continue(self):
        if self.state in ("idle", "post"):
            self.start(True)
        elif self.state == "debug_stopped":
            self.resume("continue")

    def step(self, cmd):
        if self.state in ("idle", "post"):
            self.start(True, stop_entry=True)
        elif self.state == "debug_stopped":
            self.resume(cmd)

    def toggle_bp(self):
        ed = self.cur()
        if ed:
            ed.toggle_bp(ed.getCursorPosition()[0])

    def bp_changed(self, ed, line, on):
        if ed.path and self.dsock:
            self.dbg_send({"cmd": "break", "file": ed.path, "line": line, "on": on})

    def clear_bps(self):
        for ed in self.editors():
            for ln in ed.breakpoints():
                self.bp_changed(ed, ln, False)
            ed.markerDeleteAll(BP)

    # ------------------------------------------------------------ fermeture
    def closeEvent(self, e):
        for ed in self.editors():
            if ed.isModified() or ed.unsent:
                self.pane_of(ed).setCurrentWidget(ed)
                if not self.confirm_close(ed):
                    e.ignore()
                    return
        s = self.settings
        s.setValue("geometry", self.saveGeometry())
        s.setValue("root", self.root)
        s.setValue("files", [ed.path for ed in self.editors() if ed.path and not ed.remote])
        s.setValue("layout", json.dumps(self.layout_state()))
        for key, tabs in (("bottom_order", self.bottom), ("left_order", self.left)):
            s.setValue(key, [tabs.widget(i).objectName() for i in range(tabs.count())])
        self.ssh_hub.shutdown()
        self.terminal.shutdown()
        s.setValue("vim", self.vim_on)
        s.setValue("frameless", self.frameless)
        for p in (self.proc, getattr(self, "repl_proc", None)):
            if p:
                p.blockSignals(True)
                p.kill()
                p.waitForFinished(500)
        for w in self.all_tabs():
            if isinstance(w, PdfTab):
                w.save_position()
        for w in self.all_tabs():  # les pages web doivent disparaître avant leur profil
            if isinstance(w, WebTab):
                from PyQt6 import sip
                sip.delete(w.view)
        e.accept()


def selftest():
    """Vérification de l'app empaquetée (utilisée par la fabrication des versions) : tous les
    modules se chargent et la fenêtre s'ouvre, puis se ferme toute seule. Chaque étape est
    notée dans le fichier STUDENTCODE_SELFTEST_OUT (l'exécutable Windows n'a pas de console),
    pour savoir où l'app s'est arrêtée si elle plante."""
    import faulthandler
    import importlib
    import traceback
    out_path = os.environ.get("STUDENTCODE_SELFTEST_OUT")
    log = open(out_path, "w", encoding="utf-8", buffering=1) if out_path else None
    if log is not None:
        faulthandler.enable(log)

    def say(text):
        if log is not None:
            log.write(text + "\n")
        if sys.stdout is not None:
            print(text, flush=True)

    problems, notes = [], []
    try:
        say(f"StudentCode {__version__} · Python {sys.version.split()[0]} · {sys.platform}")
        mods = ["jedi", "pyte", "paramiko", "keyring", "pypdfium2", "PyQt6.Qsci", "PyQt6.QtWebEngineWidgets",
                "PyQt6.QtSvg", "terminal", "remote", "vpn", "ai", "pdfview", "langs"]
        if WINDOWS:
            mods.append("winpty")
        for m in mods:
            try:
                importlib.import_module(m)
            except Exception as e:
                problems.append(f"module {m} : {e!r}")
        say("modules : " + ("ok" if not problems else "; ".join(problems)))
        for f in ("dbg_backend.py", "run_backend.py", "icon.svg"):
            if not os.path.isfile(os.path.join(APP_DIR, f)):
                problems.append("fichier absent : " + f)
        try:   # les données de jedi (stubs de la bibliothèque standard) sont bien embarquées
            import jedi
            names = [c.name for c in jedi.Script("import os\nos.pa", path="t.py",
                                                 environment=jedi.InterpreterEnvironment()).complete(2, 5)]
            if "path" not in names:
                notes.append("jedi : complétion vide")
        except Exception as e:
            notes.append(f"jedi : {e!r}")
        say("complétion : vérifiée")
        os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
        app = QApplication(["studentcode"])
        app.setStyleSheet(QSS)
        say("QApplication : ok")
        win = MainWindow([os.path.join(APP_DIR, "run_backend.py")])
        win.show()
        say("fenêtre : ok")
        QTimer.singleShot(1500, app.quit)
        app.exec()
        win.close()
        say("boucle Qt : ok")
        py = python_cmd()
        if not py:
            notes.append("aucun Python trouvé sur la machine")
        else:   # le Python de l'ordinateur se lance proprement depuis l'app (environnement nettoyé)
            import subprocess
            import tempfile
            script = os.path.join(tempfile.mkdtemp(), "essai.py")
            with open(script, "w") as fh:
                fh.write("print(6 * 7)\n")
            try:
                r = subprocess.run(py + ["-u", os.path.join(APP_DIR, "run_backend.py"), script],
                                   input=b"", capture_output=True, timeout=30, env=child_env(),
                                   creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
                text = (r.stdout + r.stderr).decode("utf-8", "replace")
                if "42" not in text:
                    notes.append(f"exécution Python ({' '.join(py)}) : {text[-300:]!r}")
            except Exception as e:
                notes.append(f"exécution Python ({' '.join(py)}) : {e!r}")
            say("exécution Python : " + " ".join(py))
    except BaseException:
        problems.append("plantage :\n" + traceback.format_exc())
    for n in notes:
        say("ATTENTION " + n)
    say("\n".join("ERREUR " + p for p in problems) if problems else "SELFTEST OK")
    if log is not None:
        log.close()
    os._exit(1 if problems else 0)   # sans attendre la fermeture des processus du moteur web


def main():
    prepare_children()
    if "--selftest" in sys.argv:
        if FROZEN and not WINDOWS:
            os.environ.setdefault("QTWEBENGINE_DISABLE_SANDBOX", "1")
        return selftest()
    if WINDOWS:   # icône et regroupement propres dans la barre des tâches
        try:
            import ctypes
            ctypes.windll.shell32.SetCurrentProcessExplicitAppUserModelID("StudentCode.StudentCode")
        except Exception:
            pass
    if FROZEN and not WINDOWS:
        # le bac à sable de Chromium a besoin d'un assistant installé par root, absent d'une app
        # copiée telle quelle (et souvent bloqué par AppArmor sur Ubuntu récent)
        os.environ.setdefault("QTWEBENGINE_DISABLE_SANDBOX", "1")
    env = os.environ.get("QTWEBENGINE_CHROMIUM_FLAGS", "")
    os.environ["QTWEBENGINE_CHROMIUM_FLAGS"] = (env + " " + chromium_flags()).strip()
    rules = os.environ.get("QT_LOGGING_RULES", "")
    os.environ["QT_LOGGING_RULES"] = (rules + ";" if rules else "") + QUIET_QT
    app = QApplication(sys.argv)
    app.setApplicationName("StudentCode")
    app.setDesktopFileName("studentcode")
    app.setStyle("Fusion")
    app.setStyleSheet(QSS)
    win = MainWindow(sys.argv[1:])
    win.show()
    sys.exit(app.exec())


if __name__ == "__main__":
    main()
