"""Génère les icônes de la version Rust à partir de celles de la version Python (mêmes dessins).

Chaque icône est rendue en blanc (masque, recoloré dans l'app) dans rust/assets/icons/, et la
table des icônes de fichiers est écrite dans rust/src/fileicons_gen.rs.
Usage : QT_QPA_PLATFORM=offscreen python3 scripts/gen_rust_icons.py
"""
import os, re, sys
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "studentcode"))
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
from PyQt6.QtCore import QByteArray, QRectF, Qt
from PyQt6.QtGui import QGuiApplication, QImage, QPainter
from PyQt6.QtSvg import QSvgRenderer
app = QGuiApplication([])
import icons as I

OUT = os.path.join(HERE, "..", "rust", "assets", "icons")
os.makedirs(OUT, exist_ok=True)
PX = 48

def render(body, path):
    svg = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">%s</svg>' % body
    img = QImage(PX, PX, QImage.Format.Format_ARGB32_Premultiplied)
    img.fill(Qt.GlobalColor.transparent)
    p = QPainter(img); p.setRenderHint(QPainter.RenderHint.Antialiasing); p.setRenderHint(QPainter.RenderHint.TextAntialiasing)
    QSvgRenderer(QByteArray(body and svg.encode())).render(p, QRectF(0, 0, PX, PX)); p.end()
    img.save(path)
    try:  # masques en niveaux de gris + transparence, plus légers
        from PIL import Image
        im = Image.open(path).convert("RGBA")
        if not path.endswith("python.png"):
            im = Image.merge("LA", (Image.new("L", im.size, 255), im.split()[3]))
        im.save(path, optimize=True)
    except ImportError:
        pass

names = []
for name, body in I.ICONS.items():
    if name.startswith("badge:"):
        continue
    if name == "python":
        render(body, os.path.join(OUT, "python.png"))      # en couleurs
    else:
        render(body.replace("{c}", "#ffffff"), os.path.join(OUT, name + ".png"))
    names.append(name)

# icônes propres à la version Rust
EXTRA = {
    "settings": f'<g {I.S}><path d="M12.220 2h-.440a2 2 0 0 0-2 2v.180a2 2 0 0 1-1 1.730l-.430.250a2 2 0 0 1-2 0l-.150-.080a2 2 0 0 0-2.730.730l-.220.380a2 2 0 0 0 .730 2.730l.150.100a2 2 0 0 1 1 1.720v.510a2 2 0 0 1-1 1.740l-.150.090a2 2 0 0 0-.730 2.730l.220.380a2 2 0 0 0 2.730.730l.150-.080a2 2 0 0 1 2 0l.430.250a2 2 0 0 1 1 1.730V20a2 2 0 0 0 2 2h.440a2 2 0 0 0 2-2v-.180a2 2 0 0 1 1-1.730l.430-.250a2 2 0 0 1 2 0l.150.080a2 2 0 0 0 2.730-.730l.220-.390a2 2 0 0 0-.730-2.730l-.150-.080a2 2 0 0 1-1-1.740v-.500a2 2 0 0 1 1-1.740l.150-.090a2 2 0 0 0 .730-2.730l-.220-.380a2 2 0 0 0-2.730-.730l-.150.080a2 2 0 0 1-2 0l-.430-.250a2 2 0 0 1-1-1.730V4a2 2 0 0 0-2-2z"/><circle cx="12" cy="12" r="3"/></g>',
}
for name, body in EXTRA.items():
    render(body.replace("{c}", "#ffffff"), os.path.join(OUT, name + ".png"))
    names.append(name)

# pastilles : le fond (commun) et les lettres (une image par texte), pour les teindre séparément
render('<rect x="1.5" y="3" width="21" height="18" rx="4.5" fill="#ffffff"/>', os.path.join(OUT, "badge_bg.png"))
labels = {}
def badge_text(label):
    if label not in labels:
        size = {1: 15, 2: 11.5, 3: 8.6}.get(len(label.replace("&lt;", "<").replace("&gt;", ">")), 7.5)
        fn = "badge_%d.png" % len(labels)
        render('<text x="12" y="%.1f" text-anchor="middle" font-family="DejaVu Sans, Noto Sans, sans-serif" '
               'font-weight="bold" font-size="%s" fill="#ffffff">%s</text>' % (12 + size * 0.36, size, label),
               os.path.join(OUT, fn))
        labels[label] = fn[:-4]
    return labels[label]

TN = ["#1a1b26", "#16161e", "#292e42", "#c0caf5", "#565f89", "#3b4261", "#7aa2f7", "#7dcfff", "#9ece6a",
      "#bb9af7", "#ff9e64", "#f7768e", "#e0af68", "#73daca", "#a9b1d6"]
def entry(icon_name, col):
    col = (col or "#7aa2f7").lower()
    if icon_name.startswith("badge:"):
        _, label, color, _ink = icon_name.split(":")
        return 'FileIcon::Badge("%s", "%s")' % (badge_text(label), color.lower())
    return 'FileIcon::Icon("%s", "%s")' % (icon_name, col)

lines = ["// Généré par scripts/gen_rust_icons.py à partir de studentcode/icons.py — ne pas modifier à la main.",
         "use crate::icons::FileIcon;", "",
         "/// Icônes par extension (avec le point), comme la version Python.",
         "pub const EXT: &[(&str, FileIcon)] = &["]
for ext, (n, c) in I.EXT.items():
    lines.append('    ("%s", %s),' % (ext, entry(n, c)))
lines += ["];", "", "/// Icônes par nom de fichier complet (en minuscules).", "pub const NAMES: &[(&str, FileIcon)] = &["]
for nm, (n, c) in I.NAMES.items():
    lines.append('    ("%s", %s),' % (nm, entry(n, c)))
lines += ["];", "", "/// Toutes les images d'icônes (masques blancs 48 px, sauf « python » en couleurs).",
          "pub const IMAGES: &[(&str, &[u8])] = &["]
for n in names + ["badge_bg"] + sorted(labels.values()):
    lines.append('    ("%s", include_bytes!("../assets/icons/%s.png")),' % (n, n))
lines += ["];", ""]
open(os.path.join(HERE, "..", "rust", "src", "fileicons_gen.rs"), "w").write("\n".join(lines))
print(len(names), "icônes,", len(labels), "pastilles")
