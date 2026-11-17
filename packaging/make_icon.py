"""Fabrique packaging/icon.ico (Windows) et packaging/icon.png à partir de studentcode/icon.svg."""
import io
import os
import sys

from PyQt6.QtCore import QBuffer, QIODevice, Qt
from PyQt6.QtGui import QGuiApplication, QImage, QPainter
from PyQt6.QtSvg import QSvgRenderer
from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
app = QGuiApplication(sys.argv)
svg = QSvgRenderer(os.path.join(HERE, "..", "studentcode", "icon.svg"))


def render(size):
    img = QImage(size, size, QImage.Format.Format_ARGB32)
    img.fill(Qt.GlobalColor.transparent)
    p = QPainter(img)
    p.setRenderHint(QPainter.RenderHint.Antialiasing)
    svg.render(p)
    p.end()
    buf = QBuffer()
    buf.open(QIODevice.OpenModeFlag.WriteOnly)
    img.save(buf, "PNG")
    return Image.open(io.BytesIO(bytes(buf.data()))).convert("RGBA")


big = render(256)
big.save(os.path.join(HERE, "icon.ico"), sizes=[(s, s) for s in (16, 20, 24, 32, 40, 48, 64, 128, 256)])
render(512).save(os.path.join(HERE, "icon.png"))
print("packaging/icon.ico")
