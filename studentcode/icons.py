"""Icônes vectorielles de LazyPy (dessinées en SVG, colorées à la volée)."""
from PyQt6.QtCore import QByteArray, QFileInfo, QRectF, Qt
from PyQt6.QtGui import QIcon, QPainter, QPixmap
from PyQt6.QtSvg import QSvgRenderer
from PyQt6.QtWidgets import QFileIconProvider

from theme import REMAP
_HEX = __import__('re').compile(r'#[0-9a-fA-F]{6}')


def themed(text):
    """Remplace les couleurs Tokyo Night par celles du thème courant."""
    return _HEX.sub(lambda m: REMAP.get(m.group(0).lower(), m.group(0)), text)

S = 'fill="none" stroke="{c}" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"'
F = 'fill="{c}" stroke="{c}" stroke-width="1.5" stroke-linejoin="round"'

ICONS = {
    "new": f'<g {S}><path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z"/><path d="M14 3v5h5"/><path d="M12 12v6M9 15h6"/></g>',
    "open": f'<g {S}><path d="M3 18V6a2 2 0 0 1 2-2h4l2 3h7a2 2 0 0 1 2 2v1"/><path d="M3 18l2.5-7a1.5 1.5 0 0 1 1.4-1h13.6a1 1 0 0 1 .95 1.3L19.5 18a2 2 0 0 1-1.9 1.4H4.4A1.4 1.4 0 0 1 3 18z"/></g>',
    "save": f'<g {S}><path d="M5 3h11l5 5v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z"/><path d="M7 3v5h8V3"/><rect x="7" y="13" width="10" height="8" rx="1"/></g>',
    "run": f'<path {F} d="M7 4.5v15a1 1 0 0 0 1.5.86l12.5-7.5a1 1 0 0 0 0-1.72L8.5 3.64A1 1 0 0 0 7 4.5z"/>',
    "debug": f'<g {S}><rect x="8" y="7" width="8" height="13" rx="4"/><path d="M9.5 7a2.5 2.5 0 0 1 5 0"/><path d="M12 11v9"/><path d="M8 11H4M8 15H3.5M8 18.5l-3 2M16 11h4M16 15h4.5M16 18.5l3 2M9 5L7 3M15 5l2-2"/></g>',
    "continue": f'<g><rect x="4" y="4" width="3.2" height="16" rx="1" fill="{{c}}"/><path {F} d="M11 4.8v14.4a.8.8 0 0 0 1.2.7l9-7.2a.8.8 0 0 0 0-1.4l-9-7.2a.8.8 0 0 0-1.2.7z"/></g>',
    "over": f'<g {S}><path d="M4 13a8 8 0 0 1 15.5-2.5"/><path d="M20 5v6h-6"/></g><circle cx="12" cy="18.5" r="2.3" fill="{{c}}"/>',
    "into": f'<g {S}><path d="M12 3v10"/><path d="M7.5 9l4.5 4.5L16.5 9"/></g><circle cx="12" cy="19" r="2.3" fill="{{c}}"/>',
    "outof": f'<g {S}><path d="M12 14V4"/><path d="M7.5 8L12 3.5 16.5 8"/></g><circle cx="12" cy="19" r="2.3" fill="{{c}}"/>',
    "pause": '<rect x="6" y="4.5" width="4" height="15" rx="1.2" fill="{c}"/><rect x="14" y="4.5" width="4" height="15" rx="1.2" fill="{c}"/>',
    "stop": '<rect x="5" y="5" width="14" height="14" rx="2.5" fill="{c}"/>',
    "bp": f'<circle cx="12" cy="12" r="6" fill="{{c}}"/><circle cx="12" cy="12" r="9.5" fill="none" stroke="{{c}}" stroke-width="1.5" opacity=".45"/>',
    "libs": f'<g {S}><path d="M12 2.800l8 4.400v9.600l-8 4.4-8-4.400V7.200z"/><path d="M4 7.200l8 4.5 8-4.500M12 11.700v9.500M8 5l8 4.5"/></g>',
    "search": f'<g {S}><circle cx="10.5" cy="10.5" r="6.5"/><path d="M15.5 15.500L21 21"/></g>',
    "vim": f'<g {S}><rect x="3" y="4" width="18" height="16" rx="2.5"/><path d="M7.5 9l3.5 3-3.5 3M13 15.500h4"/></g>',
    "terminal": f'<g {S}><rect x="3" y="4" width="18" height="16" rx="2.5"/><path d="M7.5 9l3.5 3-3.5 3M13 15.500h4"/></g>',
    "output": f'<g {S}><path d="M4 6h16M4 11h11M4 16h14M4 21h7"/></g>',
    "folder": '<path fill="{c}" d="M3 6.500A2.5 2.5 0 0 1 5.5 4h3.700a2 2 0 0 1 1.5.700L12.3 6.500H18.500A2.5 2.5 0 0 1 21 9v8.500a2.5 2.5 0 0 1-2.5 2.500h-13A2.5 2.5 0 0 1 3 17.500z"/>',
    "file": f'<g {S}><path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z"/><path d="M14 3v5h5"/></g>',
    "text": f'<g {S}><path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z"/><path d="M14 3v5h5M8.5 13h7M8.5 16.500h5"/></g>',
    "image": f'<g {S}><rect x="3" y="4" width="18" height="16" rx="2.5"/><circle cx="9" cy="10" r="1.8"/><path d="M4 18l5-5 3.5 3.500L16 13l4 4"/></g>',
    "menu": f'<g {S}><path d="M4 7h16M4 12h16M4 17h16"/></g>',
    "min": f'<g {S}><path d="M6 12h12"/></g>',
    "max": f'<g {S}><rect x="6" y="6" width="12" height="12" rx="2"/></g>',
    "move": f'<g {S}><path d="M3 18V6a2 2 0 0 1 2-2h4l2 3h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/><path d="M9 13.500h6M12.500 11l2.500 2.500-2.500 2.500"/></g>',
    "trash": f'<g {S}><path d="M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13M10 11v6M14 11v6"/></g>',
    "split_r": f'<g {S}><rect x="3" y="4" width="18" height="16" rx="2.5"/><path d="M12 4v16"/></g>',
    "split_d": f'<g {S}><rect x="3" y="4" width="18" height="16" rx="2.5"/><path d="M3 12h18"/></g>',
    "globe": f'<g {S}><circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3c3 3.200 3 14.800 0 18M12 3c-3 3.200-3 14.800 0 18"/></g>',
    "back": f'<g {S}><path d="M14.500 5.500L8 12l6.500 6.500"/></g>',
    "forward": f'<g {S}><path d="M9.500 5.500L16 12l-6.500 6.500"/></g>',
    "reload": f'<g {S}><path d="M20 12a8 8 0 1 1-2.600-5.900"/><path d="M20 4v5h-5"/></g>',
    "star": f'<g {S}><path d="M12 3.500l2.600 5.400 5.900.800-4.300 4.100 1 5.900L12 16.900l-5.200 2.800 1-5.900L3.500 9.700l5.900-.800z"/></g>',
    "external": f'<g {S}><path d="M14 4h6v6M20 4l-9 9M18 14v4a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h4"/></g>',
    "pdf": f'<g {S}><path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z"/><path d="M14 3v5h5"/></g><rect x="7.500" y="12" width="9" height="5.500" rx="1.200" fill="{{c}}"/>',
    "server": f'<g {S}><rect x="3" y="4" width="18" height="7" rx="2"/><rect x="3" y="13" width="18" height="7" rx="2"/><path d="M7 7.500h.01M7 16.500h.01M11 7.500h6M11 16.500h6"/></g>',
    "plug": f'<g {S}><path d="M9 3v5M15 3v5M6 8h12v3a6 6 0 0 1-12 0zM12 17v4"/></g>',
    "more": '<circle cx="5.500" cy="12" r="1.800" fill="{c}"/><circle cx="12" cy="12" r="1.800" fill="{c}"/><circle cx="18.500" cy="12" r="1.800" fill="{c}"/>',
    "edit": f'<g {S}><path d="M4 20h4L19 9l-4-4L4 16zM13.500 6.500l4 4"/></g>',
    "key": f'<g {S}><circle cx="8" cy="15" r="4"/><path d="M11 12l8-8M16 7l3 3M14 9l2 2"/></g>',
    "up": f'<g {S}><path d="M12 19V6M6 11.500L12 5.500l6 6"/></g>',
    "upload": f'<g {S}><path d="M12 15V4M7.500 8L12 3.500 16.500 8M4 15v3a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-3"/></g>',
    "download": f'<g {S}><path d="M12 4v11M7.500 11L12 15.500 16.500 11M4 15v3a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-3"/></g>',
    "shield": f'<g {S}><path d="M12 3l8 3v6c0 4.500-3.200 8-8 9-4.800-1-8-4.500-8-9V6z"/><path d="M8.500 12l2.500 2.500 4.500-5"/></g>',
    "palette": f'<g {S}><path d="M12 3a9 9 0 1 0 0 18c1.500 0 2-1 2-2 0-1.500 1-2.500 2.500-2.500H18a3 3 0 0 0 3-3c0-5.500-4-10.500-9-10.500z"/><path d="M7.500 12h.01M9.500 7.500h.01M14.500 7h.01"/></g>',
    "chev_right": f'<g fill="none" stroke="{{c}}" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><path d="M9 5.500l6.500 6.500-6.500 6.500"/></g>',
    "chev_down": f'<g fill="none" stroke="{{c}}" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><path d="M5.500 9l6.500 6.500 6.500-6.500"/></g>',
    "spark": f'<g {S}><path d="M10 3.500l1.700 4.800L16.500 10l-4.800 1.700L10 16.500l-1.700-4.800L3.500 10l4.800-1.700z"/><path d="M18 14l.900 2.100L21 17l-2.100.900L18 20l-.900-2.100L15 17l2.100-.900z"/></g>',
    "new_plus": f'<g {S}><path d="M12 6v12M6 12h12"/></g>',
    "fit_width": f'<g {S}><rect x="4" y="4" width="16" height="16" rx="2"/><path d="M7.500 12h9M9.500 10l-2 2 2 2M14.500 10l2 2-2 2"/></g>',
    "fit_page": f'<g {S}><rect x="6.500" y="3.500" width="11" height="17" rx="1.800"/><path d="M9.500 8h5M9.500 11.500h5M9.500 15h3"/></g>',
    "moon": f'<g {S}><path d="M20 14.500A8 8 0 0 1 9.500 4a8 8 0 1 0 10.500 10.500z"/></g>',
    "close": f'<g {S}><path d="M7 7l10 10M17 7L7 17"/></g>',
    # petit serpent stylisé (dessin original)
    "python": '<path fill="none" stroke="#7aa2f7" stroke-width="3.2" stroke-linecap="round" stroke-linejoin="round" d="M17.5 6.500H10a3 3 0 0 0 0 6h4a3 3 0 0 1 0 6H5.500"/>'
              '<circle cx="18.300" cy="6.500" r="2.600" fill="#e0af68"/><circle cx="18.900" cy="5.900" r=".8" fill="#1a1b26"/>',
}

_cache = {}


def pixmap(name, color, size=18, dpr=2.0):
    svg = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">%s</svg>' % \
          themed(ICONS[name].replace("{c}", color))
    pm = QPixmap(int(size * dpr), int(size * dpr))
    pm.fill(Qt.GlobalColor.transparent)
    p = QPainter(pm)
    p.setRenderHint(QPainter.RenderHint.Antialiasing)
    QSvgRenderer(QByteArray(svg.encode())).render(p, QRectF(0, 0, pm.width(), pm.height()))
    p.end()
    pm.setDevicePixelRatio(dpr)
    return pm


def icon(name, color, disabled="#3b4261", size=18):
    key = (name, color, disabled, size)
    if key not in _cache:
        ic = QIcon()
        ic.addPixmap(pixmap(name, color, size), QIcon.Mode.Normal)
        ic.addPixmap(pixmap(name, color, size), QIcon.Mode.Active)
        ic.addPixmap(pixmap(name, color, size), QIcon.Mode.Selected)
        ic.addPixmap(pixmap(name, disabled, size), QIcon.Mode.Disabled)
        _cache[key] = ic
    return _cache[key]


def badge(label, color, ink="#1a1b26"):
    """Pastille « type de fichier » : carré arrondi de couleur + lettres du langage."""
    size = {1: 15, 2: 11.5, 3: 8.6}.get(len(label), 7.5)
    name = "badge:%s:%s:%s" % (label, color, ink)
    if name not in ICONS:
        ICONS[name] = (
            '<rect x="1.5" y="3" width="21" height="18" rx="4.5" fill="%s"/>'
            '<text x="12" y="%.1f" text-anchor="middle" font-family="DejaVu Sans, Noto Sans, sans-serif" '
            'font-weight="bold" font-size="%s" fill="%s">%s</text>'
            % (color, 12 + size * 0.36, size, ink, label))
    return name


EXT = {  # extension -> (icône, couleur)
    ".py": (badge("py", "#7aa2f7"), ""), ".pyw": (badge("py", "#7aa2f7"), ""),
    ".pyi": (badge("py", "#7dcfff"), ""), ".ipynb": (badge("nb", "#ff9e64"), ""),
    ".c": (badge("C", "#7dcfff"), ""), ".h": (badge("H", "#bb9af7"), ""),
    ".cpp": (badge("C+", "#7aa2f7"), ""), ".hpp": (badge("H+", "#bb9af7"), ""),
    ".java": (badge("jv", "#f7768e"), ""), ".cs": (badge("C#", "#9ece6a"), ""),
    ".js": (badge("JS", "#e0af68"), ""), ".ts": (badge("TS", "#7aa2f7"), ""),
    ".html": (badge("&lt;&gt;", "#ff9e64"), ""), ".css": (badge("#", "#7dcfff"), ""),
    ".json": (badge("{}", "#e0af68"), ""), ".md": (badge("M↓", "#7dcfff"), ""),
    ".sh": (badge("$_", "#9ece6a"), ""), ".bash": (badge("$_", "#9ece6a"), ""),
    ".sql": (badge("db", "#bb9af7"), ""), ".rs": (badge("rs", "#ff9e64"), ""),
    ".go": (badge("go", "#7dcfff"), ""), ".lua": (badge("lua", "#7aa2f7"), ""),
    ".php": (badge("php", "#bb9af7"), ""), ".rb": (badge("rb", "#f7768e"), ""),
    ".gradle": (badge("gr", "#73daca"), ""), ".groovy": (badge("gr", "#7dcfff"), ""),
    ".kt": (badge("kt", "#bb9af7"), ""), ".kts": (badge("kt", "#73daca"), ""),
    ".swift": (badge("sw", "#ff9e64"), ""), ".dart": (badge("dt", "#7dcfff"), ""),
    ".scala": (badge("sc", "#f7768e"), ""), ".jsx": (badge("JS", "#7dcfff"), ""),
    ".tsx": (badge("TS", "#7dcfff"), ""), ".vue": (badge("vue", "#9ece6a"), ""),
    ".xml": (badge("&lt;/&gt;", "#ff9e64"), ""), ".svg": ("image", "#e0af68"),
    ".yml": (badge("yml", "#f7768e"), ""), ".yaml": (badge("yml", "#f7768e"), ""),
    ".toml": (badge("tml", "#ff9e64"), ""), ".properties": (badge("=", "#e0af68"), ""),
    ".ini": (badge("=", "#e0af68"), ""), ".cfg": (badge("=", "#e0af68"), ""), ".conf": (badge("=", "#e0af68"), ""),
    ".env": (badge("env", "#e0af68"), ""), ".bat": (badge("bat", "#9ece6a"), ""),
    ".cmd": (badge("bat", "#9ece6a"), ""), ".cmake": (badge("cm", "#7aa2f7"), ""),
    ".tex": (badge("TeX", "#73daca"), ""), ".bib": (badge("bib", "#73daca"), ""),
    ".pl": (badge("pl", "#7aa2f7"), ""), ".asm": (badge("asm", "#bb9af7"), ""),
    ".s": (badge("asm", "#bb9af7"), ""), ".diff": (badge("±", "#9ece6a"), ""),
    ".patch": (badge("±", "#9ece6a"), ""), ".vhd": (badge("hdl", "#7dcfff"), ""),
    ".v": (badge("v", "#7dcfff"), ""), ".pas": (badge("pas", "#e0af68"), ""),
    ".f90": (badge("f90", "#bb9af7"), ""), ".scss": (badge("#", "#f7768e"), ""),
    ".less": (badge("#", "#7aa2f7"), ""), ".mk": (badge("mk", "#ff9e64"), ""),
    ".lock": ("text", "#565f89"), ".gitignore": (badge("git", "#ff9e64"), ""),
    ".jar": (badge("jar", "#f7768e"), ""), ".class": (badge("cls", "#565f89"), ""),
    ".zip": (badge("zip", "#565f89"), ""), ".gz": (badge("gz", "#565f89"), ""),
    ".ovpn": ("shield", "#9ece6a"),
    ".txt": ("text", "#a9b1d6"), ".log": ("text", "#565f89"),
    ".csv": ("text", "#9ece6a"), ".toml": ("text", "#ff9e64"),
    ".yaml": ("text", "#ff9e64"), ".yml": ("text", "#ff9e64"), ".ini": ("text", "#ff9e64"),
    ".pdf": ("pdf", "#f7768e"),
    ".png": ("image", "#bb9af7"), ".jpg": ("image", "#bb9af7"), ".jpeg": ("image", "#bb9af7"),
    ".gif": ("image", "#bb9af7"), ".svg": ("image", "#bb9af7"), ".bmp": ("image", "#bb9af7"),
}


NAMES = {  # fichiers reconnus à leur nom complet
    "makefile": (badge("mk", "#ff9e64"), ""), "cmakelists.txt": (badge("cm", "#7aa2f7"), ""),
    "dockerfile": (badge("dk", "#7dcfff"), ""), "gradlew": (badge("$_", "#73daca"), ""),
    "gradlew.bat": (badge("bat", "#73daca"), ""), "jenkinsfile": (badge("gr", "#7dcfff"), ""),
    "readme": (badge("M↓", "#7dcfff"), ""), "license": ("text", "#e0af68"),
    ".gitignore": (badge("git", "#ff9e64"), ""), ".env": (badge("env", "#e0af68"), ""),
}


def file_icon(path):
    import os
    base = os.path.basename(path).lower()
    name, col = NAMES.get(base) or EXT.get(os.path.splitext(base)[1] or base, ("file", "#565f89"))
    return icon(name, col or "#7aa2f7")


class IconProvider(QFileIconProvider):
    def icon(self, arg):
        if isinstance(arg, QFileInfo):
            if arg.isDir():
                return icon("folder", "#7aa2f7")
            return file_icon(arg.fileName())
        if arg == QFileIconProvider.IconType.Folder:
            return icon("folder", "#7aa2f7")
        return icon("file", "#565f89")
