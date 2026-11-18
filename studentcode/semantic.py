"""Coloration enrichie pour les langages à accolades (C, C++, Java, C#, JavaScript…) :
en plus des mots-clés, les types, les fonctions, les constantes en MAJUSCULES et les fichiers
inclus ont chacun leur couleur, comme dans VS Code.

Le lexer de Scintilla ne connaît que des listes de mots : on les remplit à partir du texte
(noms suivis d'une parenthèse → fonctions, noms utilisés dans une déclaration → types), et les
constantes passent par un indicateur qui change la couleur du texte."""
import re

from PyQt6.Qsci import QsciLexerCPP, QsciScintilla
from PyQt6.QtCore import QTimer
from PyQt6.QtGui import QColor

B = QsciScintilla
# styles du lexer C/C++ (communs à Java, C#, JavaScript)
S_PREPRO, S_IDENT, S_TYPES, S_FUNCS = 9, 11, 16, 19
IND_CONST, IND_INCLUDE = 12, 13
MAX_BYTES = 600_000          # au-delà, on garde la coloration simple (gros fichiers générés)

IDENT = re.compile(rb"\b[A-Za-z_]\w*\b")
CALL = re.compile(rb"\b([A-Za-z_]\w*)\s*\(")
CONST = re.compile(rb"\b(?=\w*[A-Z])[A-Z_][A-Z0-9_]+\b")
INCLUDE = re.compile(rb"^[ \t]*#[ \t]*(?:include|import)[ \t]*(<[^>\n]*>)", re.M)
# déclarations : « struct Nom », « Type* nom », « Type nom; », « } Nom; » (typedef struct)
DECL_KW = re.compile(rb"\b(?:struct|enum|union|class|interface|record|typedef\s+(?:struct|enum|union))\s+([A-Za-z_]\w*)")
TYPEDEF_END = re.compile(rb"\btypedef\b[^;{]*?\b([A-Za-z_]\w*)\s*;")
TYPEDEF_BLOCK = re.compile(rb"\}\s*([A-Za-z_]\w*)\s*;")
PTR_DECL = re.compile(rb"\b([A-Z]\w*)\s*[*&]+\s*[A-Za-z_]")
VAR_DECL = re.compile(rb"(?:^|[(,;{]|\bconst|\bstatic)\s*([A-Za-z_]\w*)\s+\**\s*[A-Za-z_]\w*\s*(?:[,;=)\[]|$)", re.M)
CAMEL_TYPE = re.compile(rb"\b[A-Z](?=\w*[a-z])\w*\b")

C_TYPES = (b"size_t ssize_t ptrdiff_t intptr_t uintptr_t off_t time_t clock_t FILE va_list wchar_t "
           b"int8_t int16_t int32_t int64_t uint8_t uint16_t uint32_t uint64_t bool string vector map "
           b"set list pair unique_ptr shared_ptr Vector2 Vector3 Vector4 Matrix Color Rectangle Image "
           b"Texture Texture2D RenderTexture2D Font Camera Camera2D Camera3D Mesh Model Shader Material "
           b"Ray BoundingBox Sound Music Wave").split()
CONSTS = {b"NULL", b"nullptr", b"true", b"false", b"EOF", b"null", b"undefined", b"None"}
# langages où, par convention, un nom en CamelCase est une classe
CAMEL_LANGS = {"java", "csharp", "kotlin", "scala", "groovy", "dart", "swift", "typescript", "javascript"}


class Semantic:
    def __init__(self, editor, key, colors):
        self.ed, self.key = editor, key
        lx = editor.lexer()
        self.base = {s: (lx.keywords(s) or "").encode() for s in (2, 4)}
        self.sets = None
        if hasattr(lx, "setStylePreprocessor"):   # « #include "x.h" » : la chaîne garde sa couleur
            lx.setStylePreprocessor(True)
        lx.setColor(QColor(colors["prepro"]), S_PREPRO)
        lx.setColor(QColor(colors["type"]), S_TYPES)
        lx.setColor(QColor(colors["func"]), S_FUNCS)
        for ind, col in ((IND_CONST, colors["const"]), (IND_INCLUDE, colors["include"])):
            editor.indicatorDefine(B.IndicatorStyle.TextColorIndicator, ind)
            editor.setIndicatorForegroundColor(QColor(col), ind)
        self.timer = QTimer(editor)
        self.timer.setSingleShot(True)
        self.timer.setInterval(350)
        self.timer.timeout.connect(self.refresh)
        editor.textChanged.connect(self.timer.start)
        self.refresh()

    @staticmethod
    def supported(lexer):
        return isinstance(lexer, QsciLexerCPP)

    def refresh(self):
        ed = self.ed
        if ed.lexer() is None or not self.supported(ed.lexer()):
            return
        data = ed.text().encode("utf-8")
        if len(data) > MAX_BYTES:
            return
        keywords = set((ed.lexer().keywords(1) or "").encode().split())
        funcs = {m.group(1) for m in CALL.finditer(data)} - keywords
        types = set(C_TYPES) if self.key == "c" else set()
        for rx in (DECL_KW, TYPEDEF_END, TYPEDEF_BLOCK, PTR_DECL, VAR_DECL):
            types.update(m.group(1) for m in rx.finditer(data))
        if self.key in CAMEL_LANGS:
            types.update(m.group(0) for m in CAMEL_TYPE.finditer(data))
        types -= keywords | {b"return", b"else", b"case", b"goto", b"new", b"delete", b"throw", b"sizeof"}
        types = {t for t in types if not CONST.fullmatch(t)}   # MAJUSCULES : des constantes
        funcs -= types
        sets = (b" ".join(sorted(types | set(self.base[2].split()))),
                b" ".join(sorted(funcs | set(self.base[4].split()))))
        if sets != self.sets:   # ne recolore tout le fichier que si les listes ont changé
            self.sets = sets
            ed.SendScintilla(B.SCI_SETKEYWORDS, 1, sets[0])
            ed.SendScintilla(B.SCI_SETKEYWORDS, 3, sets[1])
            ed.SendScintilla(B.SCI_COLOURISE, 0, -1)
        else:
            ed.SendScintilla(B.SCI_COLOURISE, 0, -1)
        self.paint(data)

    def paint(self, data):
        ed = self.ed
        n = len(data)
        for ind in (IND_CONST, IND_INCLUDE):
            ed.SendScintilla(B.SCI_SETINDICATORCURRENT, ind)
            ed.SendScintilla(B.SCI_INDICATORCLEARRANGE, 0, n)
        ed.SendScintilla(B.SCI_SETINDICATORCURRENT, IND_CONST)
        for m in IDENT.finditer(data):
            word = m.group(0)
            if (word in CONSTS or CONST.fullmatch(word)) and \
                    ed.SendScintilla(B.SCI_GETSTYLEAT, m.start()) in (S_IDENT, S_PREPRO):
                ed.SendScintilla(B.SCI_INDICATORFILLRANGE, m.start(), len(word))
        ed.SendScintilla(B.SCI_SETINDICATORCURRENT, IND_INCLUDE)
        for m in INCLUDE.finditer(data):
            ed.SendScintilla(B.SCI_INDICATORFILLRANGE, m.start(1), m.end(1) - m.start(1))
