"""Langages reconnus par l'éditeur : détection par nom de fichier, coloration et commentaires.

La coloration s'appuie sur les lexers de QScintilla (45 langages). Les langages qu'il ne
connaît pas (Gradle/Groovy, Kotlin, Rust, Go, TypeScript…) partent du plus proche, avec
leurs propres mots-clés. Les couleurs ne sont pas réglées langage par langage : chaque
style est classé d'après sa description (commentaire, chaîne, nombre…) et reçoit la
couleur correspondante du thème. Tout langage reçoit ainsi la même palette.
"""
import os

from PyQt6 import Qsci
from PyQt6.QtGui import QColor, QFont

# ------------------------------------------------------------- mots-clés
JAVA = ("abstract assert boolean break byte case catch char class const continue default do double "
        "else enum extends final finally float for goto if implements import instanceof int interface "
        "long native new package private protected public return short static strictfp super switch "
        "synchronized this throw throws transient try void volatile while var record sealed permits "
        "yield true false null")
GROOVY = JAVA + " def as in trait it println"
GRADLE_DSL = ("plugins id version apply dependencies repositories implementation api compileOnly "
              "runtimeOnly testImplementation testRuntimeOnly testCompileOnly annotationProcessor "
              "mavenCentral mavenLocal google gradlePluginPortal maven url application mainClass "
              "java toolchain languageVersion JavaLanguageVersion sourceCompatibility targetCompatibility "
              "tasks task register named withType doLast doFirst dependsOn test useJUnitPlatform jar "
              "manifest attributes from into include exclude sourceSets main resources srcDir srcDirs "
              "group description ext buildscript allprojects subprojects project rootProject "
              "settings pluginManagement dependencyResolutionManagement libs platform enforcedPlatform "
              "kotlin android defaultConfig compileSdk minSdk targetSdk namespace wrapper "
              "gradleVersion distributionType run standardInput args jvmArgs systemProperty")
KOTLIN = ("as break class continue do else false for fun if in interface is null object package return "
          "super this throw true try typealias typeof val var when while by catch constructor delegate "
          "dynamic field file finally get import init param property receiver set setparam where "
          "actual abstract annotation companion const crossinline data enum expect external final infix "
          "inline inner internal lateinit noinline open operator out override private protected public "
          "reified sealed suspend tailrec vararg it")
KOTLIN_TYPES = ("Int Long Short Byte Double Float Boolean Char String Unit Any Nothing Array List "
                "MutableList Map MutableMap Set MutableSet Pair Triple println print listOf mutableListOf "
                "mapOf mutableMapOf setOf arrayOf lazy require check error")
RUST = ("as async await break const continue crate dyn else enum extern false fn for if impl in let loop "
        "match mod move mut pub ref return self Self static struct super trait true type unsafe use "
        "where while union macro_rules")
RUST_TYPES = ("i8 i16 i32 i64 i128 isize u8 u16 u32 u64 u128 usize f32 f64 bool char str String Vec "
              "Option Some None Result Ok Err Box Rc Arc RefCell HashMap HashSet BTreeMap println print "
              "format vec panic assert assert_eq eprintln write writeln")
GO = ("break case chan const continue default defer else fallthrough for func go goto if import "
      "interface map package range return select struct switch type var true false nil iota")
GO_TYPES = ("bool byte complex64 complex128 error float32 float64 int int8 int16 int32 int64 rune string "
            "uint uint8 uint16 uint32 uint64 uintptr any append cap close copy delete len make new panic "
            "print println recover fmt")
TS = ("break case catch class const continue debugger default delete do else enum export extends false "
      "finally for function if import in instanceof new null return super switch this throw true try "
      "typeof var void while with as implements interface let package private protected public static "
      "yield async await type namespace declare abstract readonly keyof infer is from of get set "
      "satisfies module require undefined")
TS_TYPES = ("string number boolean any unknown never void object symbol bigint Array Promise Record "
            "Partial Readonly Map Set Date Error JSON Math console window document")
SWIFT = ("associatedtype class deinit enum extension fileprivate func import init inout internal let open "
         "operator private protocol public rethrows static struct subscript typealias var break case "
         "continue default defer do else fallthrough for guard if in repeat return switch where while "
         "as catch false is nil self Self super throw throws true try async await some any")
DART = ("abstract as assert async await break case catch class const continue covariant default deferred "
        "do dynamic else enum export extends extension external factory false final finally for get if "
        "implements import in interface is late library mixin new null on operator part required "
        "rethrow return sealed set show static super switch sync this throw true try typedef var void "
        "while with yield")
SCALA = ("abstract case catch class def do else extends false final finally for forSome if implicit "
         "import lazy match new null object override package private protected return sealed super this "
         "throw trait try true type val var while with yield given using enum then export")
LATEX = ("documentclass usepackage begin end section subsection subsubsection paragraph chapter part "
         "title author date maketitle tableofcontents textbf textit emph underline texttt textsc item "
         "label ref eqref cite pageref caption includegraphics centering newpage clearpage newline "
         "hline vspace hspace footnote url href input include bibliography bibliographystyle frac sqrt "
         "sum int prod lim left right cdot times leq geq neq infty alpha beta gamma delta epsilon lambda "
         "mu pi sigma theta omega mathbf mathrm mathcal mathbb text newcommand renewcommand def let "
         "setlength usetikzlibrary draw node fill path small large Large LARGE huge Huge tiny footnotesize "
         "normalsize bf it tt rm sf noindent indent par linewidth textwidth hfill vfill appendix abstract "
         "verb lstinline lstinputlisting minted")
DOCKER = ("FROM RUN CMD LABEL EXPOSE ENV ADD COPY ENTRYPOINT VOLUME USER WORKDIR ARG ONBUILD STOPSIGNAL "
          "HEALTHCHECK SHELL MAINTAINER AS")

# clé -> (lexer de base, mots-clés {ensemble: texte} ou None, commentaire (début, fin))
SLASH, HASH = ("//", ""), ("#", "")
LANGS = {
    "c": ("QsciLexerCPP", None, SLASH),
    "java": ("QsciLexerJava", None, SLASH),
    "groovy": ("QsciLexerJava", {1: GROOVY, 2: GRADLE_DSL}, SLASH),
    "kotlin": ("QsciLexerJava", {1: KOTLIN, 2: KOTLIN_TYPES + " " + GRADLE_DSL}, SLASH),
    "csharp": ("QsciLexerCSharp", None, SLASH),
    "rust": ("QsciLexerCPP", {1: RUST, 2: RUST_TYPES}, SLASH),
    "go": ("QsciLexerCPP", {1: GO, 2: GO_TYPES}, SLASH),
    "swift": ("QsciLexerCPP", {1: SWIFT}, SLASH),
    "dart": ("QsciLexerCPP", {1: DART}, SLASH),
    "scala": ("QsciLexerJava", {1: SCALA}, SLASH),
    "javascript": ("QsciLexerJavaScript", None, SLASH),
    "typescript": ("QsciLexerJavaScript", {1: TS, 2: TS_TYPES}, SLASH),
    "d": ("QsciLexerD", None, SLASH),
    "json": ("QsciLexerJSON", None, SLASH),
    "html": ("QsciLexerHTML", None, ("<!--", "-->")),
    "xml": ("QsciLexerXML", None, ("<!--", "-->")),
    "markdown": ("QsciLexerMarkdown", None, ("<!--", "-->")),
    "css": ("QsciLexerCSS", None, ("/*", "*/")),
    "scss": ("QsciLexerCSS", None, SLASH),
    "yaml": ("QsciLexerYAML", None, HASH),
    "props": ("QsciLexerProperties", None, HASH),
    "bash": ("QsciLexerBash", None, HASH),
    "docker": ("QsciLexerBash", {1: DOCKER}, HASH),
    "batch": ("QsciLexerBatch", None, ("REM", "")),
    "cmake": ("QsciLexerCMake", None, HASH),
    "make": ("QsciLexerMakefile", None, HASH),
    "sql": ("QsciLexerSQL", None, ("--", "")),
    "lua": ("QsciLexerLua", None, ("--", "")),
    "perl": ("QsciLexerPerl", None, HASH),
    "ruby": ("QsciLexerRuby", None, HASH),
    "tcl": ("QsciLexerTCL", None, HASH),
    "coffee": ("QsciLexerCoffeeScript", None, HASH),
    "tex": ("QsciLexerTeX", {1: LATEX}, ("%", "")),
    "matlab": ("QsciLexerMatlab", None, ("%", "")),
    "octave": ("QsciLexerOctave", None, ("%", "")),
    "postscript": ("QsciLexerPostScript", None, ("%", "")),
    "pascal": ("QsciLexerPascal", None, SLASH),
    "fortran": ("QsciLexerFortran", None, ("!", "")),
    "fortran77": ("QsciLexerFortran77", None, ("C", "")),
    "vhdl": ("QsciLexerVHDL", None, ("--", "")),
    "verilog": ("QsciLexerVerilog", None, SLASH),
    "asm": ("QsciLexerNASM", None, (";", "")),
    "masm": ("QsciLexerMASM", None, (";", "")),
    "diff": ("QsciLexerDiff", None, None),
    "po": ("QsciLexerPO", None, HASH),
    "pov": ("QsciLexerPOV", None, SLASH),
    "spice": ("QsciLexerSpice", None, ("*", "")),
    "idl": ("QsciLexerIDL", None, SLASH),
    "avs": ("QsciLexerAVS", None, ("#", "")),
    "intelhex": ("QsciLexerIntelHex", None, None),
    "srec": ("QsciLexerSRec", None, None),
    "tekhex": ("QsciLexerTekHex", None, None),
}

EXTENSIONS = {
    "c": ".c .h .cpp .hpp .cc .cxx .hh .hxx .ino .cu .m .mm",
    "java": ".java .jsp",
    "groovy": ".gradle .groovy .gvy .gy .jenkinsfile",
    "kotlin": ".kt .kts",
    "csharp": ".cs",
    "rust": ".rs",
    "go": ".go",
    "swift": ".swift",
    "dart": ".dart",
    "scala": ".scala .sc .sbt",
    "javascript": ".js .mjs .cjs .jsx",
    "typescript": ".ts .tsx .mts .cts",
    "d": ".d",
    "json": ".json .jsonc .json5 .geojson .ipynb .webmanifest .babelrc .eslintrc",
    "html": ".html .htm .xhtml .php .phtml .vue .svelte .twig .ejs .hbs",
    "xml": ".xml .svg .xsd .xsl .xslt .plist .iml .fxml .pom .xaml .csproj .wsdl .rss .kml",
    "markdown": ".md .markdown .mdx",
    "css": ".css",
    "scss": ".scss .less .sass",
    "yaml": ".yml .yaml",
    "props": (".properties .toml .ini .cfg .conf .env .editorconfig .gitattributes .gitignore "
              ".dockerignore .gitmodules .desktop .service .npmrc .flake8 .pylintrc .lock .tf .hcl"),
    "bash": ".sh .bash .zsh .ksh .fish .command .bashrc .zshrc .profile .bash_profile .bash_aliases",
    "docker": ".dockerfile .containerfile",
    "batch": ".bat .cmd",
    "cmake": ".cmake",
    "make": ".mk .mak",
    "sql": ".sql .psql .pgsql .mysql .ddl",
    "lua": ".lua",
    "perl": ".pl .pm .t",
    "ruby": ".rb .rake .gemspec .ru",
    "tcl": ".tcl .tk",
    "coffee": ".coffee",
    "tex": ".tex .sty .cls .bib .ltx",
    "matlab": ".mat",
    "octave": ".oct",
    "postscript": ".ps .eps",
    "pascal": ".pas .pp .inc .dpr .lpr",
    "fortran": ".f90 .f95 .f03 .f08",
    "fortran77": ".f .for .f77",
    "vhdl": ".vhd .vhdl",
    "verilog": ".v .sv .svh .vh",
    "asm": ".asm .nasm .s .S",
    "masm": ".masm",
    "diff": ".diff .patch",
    "po": ".po .pot",
    "pov": ".pov",
    "spice": ".cir .sp .spice .net",
    "idl": ".idl",
    "avs": ".avs .avsi",
    "intelhex": ".hex .ihex",
    "srec": ".srec .s19 .s28 .s37 .mot",
    "tekhex": ".tek",
}
EXT_LANG = {e.lower(): k for k, exts in EXTENSIONS.items() for e in exts.split()}
NAMES = {  # fichiers reconnus à leur nom complet
    "makefile": "make", "gnumakefile": "make", "cmakelists.txt": "cmake", "dockerfile": "docker",
    "containerfile": "docker", "gradlew": "bash", "mvnw": "bash", "jenkinsfile": "groovy",
    "gemfile": "ruby", "rakefile": "ruby", "vagrantfile": "ruby", "podfile": "ruby",
    "pipfile": "props", "procfile": "props", "requirements.txt": "props", ".env": "props",
    "pkgbuild": "bash", "configure": "bash", "readme": "markdown",
}
PYTHON_EXT = (".py", ".pyw", ".pyi")


def detect(path):
    """Clé du langage d'un fichier (None : Python, texte brut ou inconnu)."""
    if not path:
        return None
    name = os.path.basename(path).lower()
    if name in NAMES:
        return NAMES[name]
    if name.startswith("dockerfile."):
        return "docker"
    if name.startswith(".env."):
        return "props"
    ext = os.path.splitext(name)[1] or (name if name.startswith(".") else "")
    return EXT_LANG.get(ext)


def comment_marks(path):
    """(début, fin) d'un commentaire de ligne pour ce fichier ; None si le format n'en a pas."""
    if not path or path.lower().endswith(PYTHON_EXT):
        return ("#", "")
    key = detect(path)
    return LANGS[key][2] if key else ("#", "")


_classes = {}


def lexer_class(key):
    base_name, kw, _ = LANGS[key]
    if key in _classes:
        return _classes[key]
    base = getattr(Qsci, base_name)
    if kw:
        def keywords(self, kset, _kw=kw, _base=base):
            return _kw[kset] if kset in _kw else _base.keywords(self, kset)
        cls = type("Lexer_" + key, (base,), {"keywords": keywords})
    else:
        cls = base
    _classes[key] = cls
    return cls


def make_lexer(key, parent=None):
    lx = lexer_class(key)(parent)
    for opt in ("setFoldComments", "setFoldCompact"):
        if hasattr(lx, opt):
            getattr(lx, opt)(opt == "setFoldComments")
    return lx


def keyword_list(lx):
    words = set()
    for s in range(1, 9):
        try:
            text = lx.keywords(s)
        except Exception:
            text = None
        if text:
            words.update(w for w in text.split() if len(w) > 2 and w.replace("_", "").isalnum())
    return sorted(words)


# ---------------------------------------------------------------- couleurs
def classify(desc, P):
    """Couleur, italique, gras et fond pour un style, d'après sa description."""
    d = desc.lower()

    def has(*words):
        return any(w in d for w in words)
    if not d or d in ("default", "html default", "whitespace", "identifier", "text", "plain text"):
        return None
    if has("inactive"):
        return P["GUTTER"], False, False, None
    if has("error", "unclosed", "unknown", "bad ", "invalid"):
        return P["RED"], False, False, None
    if has("task marker"):
        return P["YELLOW"], True, True, None
    if has("doc keyword", "javadoc keyword", "comment keyword", "doc comment keyword"):
        return P["TEAL"], True, False, None
    if has("comment", "remark"):
        return P["COMMENT"], True, False, None
    if has("added", "inserted"):
        return P["GREEN"], False, False, None
    if has("removed", "deleted"):
        return P["RED"], False, False, None
    if has("changed", "modified"):
        return P["YELLOW"], False, False, None
    if has("header", "heading", "section", "title", "chunk"):
        return P["BLUE"], False, True, None
    if has("strong"):
        return P["ORANGE"], False, True, None
    if has("emphasis"):
        return P["MAGENTA"], True, False, None
    if has("link", "url", "iri", "uri"):
        return P["CYAN"], False, False, None
    if has("code block"):
        return P["GREEN"], False, False, P["BG_HL"]
    if has("code between", "pre-char"):
        return P["GREEN"], False, False, None
    if has("list item", "horizontal rule", "block quote"):
        return P["YELLOW"], False, False, None
    if has("regular expression", "regex"):
        return P["TEAL"], False, False, None
    if has("escape"):
        return P["CYAN"], False, False, None
    if has("number", "numeric", "float", "integer", "user-defined literal", "byte count", "checksum"):
        return P["ORANGE"], False, False, None
    if has("string", "character", "quoted", "here document", "heredoc", "cdata", "text block",
           "backtick", "verbatim", "value", "data", "literal"):
        return P["GREEN"], False, False, None
    if has("pre-processor", "preprocessor", "directive", "decorator", "annotation", "pragma", "include",
           "macro", "define"):
        return P["BLUE"], False, False, None
    if has("register"):
        return P["ORANGE"], False, False, None
    if has("label", "target", "address", "record"):
        return P["YELLOW"], False, False, None
    if has("secondary keyword", "keyword set 2", "global class", "typedef", "type", "class name",
           "class", "user defined", "user-defined", "struct"):
        return P["YELLOW"], False, False, None
    if has("function", "procedure", "method", "builtin", "built-in", "standard", "library",
           "intrinsic", "system", "command", "subroutine", "module"):
        return P["BLUE"], False, False, None
    if has("keyword", "reserved", "statement", "instruction", "cpu", "directive"):
        return P["MAGENTA"], False, False, None
    if has("operator", "punctuation", "delimiter", "assignment", "brace", "bracket", "separator",
           "end of a tag", "special"):
        return P["CYAN"], False, False, None
    if has("tag"):
        return P["RED"], False, False, None
    if has("property", "key", "json"):
        return P["BLUE"], False, False, None
    if has("attribute", "parameter", "argument", "option"):
        return P["YELLOW"], False, False, None
    if has("entity", "constant", "symbol", "atom"):
        return P["ORANGE"], False, False, None
    if has("variable", "scalar", "array", "hash", "reference", "expansion", "substitution", "field"):
        return P["BLUE"], False, False, None
    if has("document", "marker", "fragment"):
        return P["MAGENTA"], False, False, None
    return None


def theme_lexer(lx, font, P):
    """Applique police et palette du thème à tous les styles du lexer."""
    lx.setDefaultFont(font)
    lx.setDefaultPaper(QColor(P["BG"]))
    lx.setDefaultColor(QColor(P["FG"]))
    lx.setFont(font)
    lx.setPaper(QColor(P["BG"]))
    lx.setColor(QColor(P["FG"]))
    props = isinstance(lx, Qsci.QsciLexerProperties)
    for style in range(256):
        desc = lx.description(style)
        res = classify(desc, P) if desc else None
        if props and style == 0:      # .properties, .toml, .ini : le texte « par défaut » est la valeur
            res = (P["GREEN"], False, False, None)
        if not res:
            continue
        color, italic, bold, paper = res
        lx.setColor(QColor(color), style)
        if italic or bold:
            f = QFont(font)
            f.setItalic(italic)
            f.setBold(bold)
            lx.setFont(f, style)
        if paper:
            lx.setPaper(QColor(paper), style)
            lx.setEolFill(True, style)
    return lx
