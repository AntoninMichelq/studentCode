"""Lecteur PDF de SCode, pensé pour la place : la page occupe tout l'onglet, et une
barre d'outils discrète flotte en haut à droite, visible seulement quand la souris bouge.

Rendu par PDFium (bibliothèque pypdfium2) : défilement continu, ajustement à la largeur,
zoom (Ctrl+molette), recherche (Ctrl+F), sélection et copie du texte, liens cliquables,
mode sombre, et la page lue est retenue pour la prochaine ouverture.
"""
import ctypes
import json
import os

from PyQt6.QtCore import QPoint, QPointF, QRect, QRectF, QSettings, QSize, Qt, QTimer, QUrl, pyqtSignal
from PyQt6.QtGui import QColor, QDesktopServices, QFont, QImage, QPainter, QPen
from PyQt6.QtWidgets import (QApplication, QFrame, QHBoxLayout, QLabel, QLineEdit, QScrollArea,
                             QToolButton, QVBoxLayout, QWidget)

from icons import icon
from theme import BG, BG_DARK, BG_HL, BLUE, COMMENT, FG, GREEN, LIGHT, YELLOW

from lazy import LazyModule, available

HAS_PDF = available("pypdfium2")
pdfium = LazyModule("pypdfium2")        # chargés à l'ouverture du premier PDF
praw = LazyModule("pypdfium2.raw")

GAP = 10            # espace entre deux pages (pixels)
MARGIN = 6          # marge autour des pages
ZOOMS = [0.5, 0.67, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0, 4.0]
CACHE_MAX = 6       # pages rendues gardées en mémoire (une page ≈ 4 Mo à l'écran)


def _settings():
    return QSettings("StudentCode", "StudentCode")


def load_positions():
    try:
        v = json.loads(_settings().value("pdf_positions", "") or "{}")
        return v if isinstance(v, dict) else {}
    except ValueError:
        return {}


class Canvas(QWidget):
    """Les pages, empilées verticalement, dessinées à la demande."""

    def __init__(self, tab):
        super().__init__()
        self.tab = tab
        self.setMouseTracking(True)
        self.setFocusPolicy(Qt.FocusPolicy.StrongFocus)
        self.sel = None            # (page, index début, index fin) dans le texte de la page
        self.sel_anchor = None
        self.press = None

    # --------------------------------------------------------------- dessin
    def paintEvent(self, e):
        t = self.tab
        p = QPainter(self)
        p.fillRect(e.rect(), QColor(BG_DARK if not LIGHT else BG_HL))
        missing = []
        for i in t.visible_pages(e.rect()):
            r = t.page_rect(i)
            img = t.cached(i)
            if img is None:
                p.fillRect(r, QColor("#2a2a2a" if t.dark else "#ffffff"))
                missing.append(i)
            else:
                p.drawImage(QRectF(r), img)
            self.paint_marks(p, i, r)
        p.end()
        if missing:
            t.queue(missing)

    def paint_marks(self, p, i, r):
        t = self.tab
        found = [b for (pg, b) in t.matches if pg == i]
        for n, box in enumerate(found):
            current = t.match_pos >= 0 and t.matches[t.match_pos] == (i, box)
            c = QColor(YELLOW if not current else "#ff9e64")
            c.setAlpha(110)
            p.fillRect(t.to_screen(i, box), c)
        if self.sel and self.sel[0] == i:
            c = QColor(BLUE)
            c.setAlpha(90)
            for box in t.text_rects(i, self.sel[1], self.sel[2]):
                p.fillRect(t.to_screen(i, box), c)

    # --------------------------------------------------------------- souris
    def mousePressEvent(self, e):
        self.setFocus()
        t = self.tab
        if e.button() != Qt.MouseButton.LeftButton:
            return
        pos = e.position()
        hit = t.page_at(pos)
        if hit is None:
            self.sel = None
            self.update()
            return
        i, x, y = hit
        link = t.link_at(i, x, y)
        if link is not None:
            self.press = ("link", link)
            return
        idx = t.char_at(i, x, y)
        self.sel_anchor = (i, idx)
        self.sel = None
        self.press = ("select", pos)
        self.update()

    def mouseMoveEvent(self, e):
        t = self.tab
        t.poke_toolbar()
        hit = t.page_at(e.position())
        if self.press and self.press[0] == "select" and self.sel_anchor:
            if hit and hit[0] == self.sel_anchor[0]:
                idx = t.char_at(*hit)
                a = self.sel_anchor[1]
                if a >= 0 and idx >= 0:
                    self.sel = (hit[0], min(a, idx), max(a, idx) + 1)
                    self.update()
            return
        on_link = hit is not None and t.link_at(*hit) is not None
        on_text = hit is not None and not on_link and t.char_at(*hit, near=True) >= 0
        self.setCursor(Qt.CursorShape.PointingHandCursor if on_link else
                       Qt.CursorShape.IBeamCursor if on_text else Qt.CursorShape.ArrowCursor)

    def mouseReleaseEvent(self, e):
        press, self.press = self.press, None
        if press and press[0] == "link":
            self.tab.follow(press[1])
        elif self.sel:
            text = self.tab.selected_text()
            cb = QApplication.clipboard()
            if text and cb.supportsSelection():
                cb.setText(text, cb.Mode.Selection)

    def mouseDoubleClickEvent(self, e):
        hit = self.tab.page_at(e.position())
        if hit:
            i, x, y = hit
            idx = self.tab.char_at(i, x, y)
            if idx >= 0:
                a, b = self.tab.word_bounds(i, idx)
                self.sel = (i, a, b)
                self.update()

    def wheelEvent(self, e):
        if e.modifiers() & Qt.KeyboardModifier.ControlModifier:
            self.tab.zoom_step(1 if e.angleDelta().y() > 0 else -1, anchor=e.position())
            e.accept()
        else:
            e.ignore()   # la zone de défilement s'en charge

    def keyPressEvent(self, e):
        t, k = self.tab, e.key()
        ctrl = e.modifiers() & Qt.KeyboardModifier.ControlModifier
        if ctrl and k == Qt.Key.Key_C:
            t.copy()
        elif ctrl and k == Qt.Key.Key_A:
            self.sel = (t.current_page(), 0, t.textpage(t.current_page()).count_chars())
            self.update()
        elif ctrl and k == Qt.Key.Key_F:
            t.show_search()
        elif ctrl and k in (Qt.Key.Key_Plus, Qt.Key.Key_Equal):
            t.zoom_step(1)
        elif ctrl and k == Qt.Key.Key_Minus:
            t.zoom_step(-1)
        elif ctrl and k == Qt.Key.Key_0:
            t.set_fit(True)
        elif k in (Qt.Key.Key_Home, Qt.Key.Key_End):
            t.go_page(0 if k == Qt.Key.Key_Home else t.count - 1)
        elif k in (Qt.Key.Key_Right, Qt.Key.Key_PageDown) and t.fit_page:
            t.go_page(t.current_page() + 1)
        elif k in (Qt.Key.Key_Left, Qt.Key.Key_PageUp) and t.fit_page:
            t.go_page(t.current_page() - 1)
        elif k == Qt.Key.Key_Escape:
            self.sel = None
            t.hide_search()
            self.update()
        else:
            super().keyPressEvent(e)


class PdfTab(QWidget):
    """Onglet de lecture d'un PDF."""
    page_changed = pyqtSignal(int)

    def __init__(self, win, path):
        super().__init__()
        self.win = win
        self.path = path
        self.doc = pdfium.PdfDocument(path)
        self.count = len(self.doc)
        self.sizes = [self.doc[i].get_size() for i in range(self.count)]
        self.widest = max((w for w, h in self.sizes), default=595)
        self.cache = {}            # page -> (échelle, QImage)
        self.textpages = {}
        self.links = {}
        self.todo = []
        self.matches = []          # [(page, (gauche, bas, droite, haut))]
        self.match_pos = -1
        self.last_search = ""
        pos = load_positions().get(path, {})
        self.fit = pos.get("fit", True)
        self.fit_page = False
        self.scale = float(pos.get("scale", 1.0))
        self.dark = _settings().value("pdf_dark", False, type=bool)
        self.offsets = []

        self.area = QScrollArea()
        self.area.setFrameShape(QFrame.Shape.NoFrame)
        self.area.setWidgetResizable(False)
        self.area.setAlignment(Qt.AlignmentFlag.AlignHCenter)
        self.area.setStyleSheet(f"QScrollArea {{ background: {BG_DARK if not LIGHT else BG_HL}; }}")
        self.canvas = Canvas(self)
        self.area.setWidget(self.canvas)
        self.area.viewport().installEventFilter(self)
        self.area.verticalScrollBar().valueChanged.connect(self.scrolled)
        lay = QVBoxLayout(self)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.addWidget(self.area)
        self.setFocusProxy(self.canvas)
        self.build_toolbar()
        self.render_timer = QTimer(self)
        self.render_timer.setSingleShot(True)
        self.render_timer.timeout.connect(self.render_some)
        self.save_timer = QTimer(self)
        self.save_timer.setSingleShot(True)
        self.save_timer.setInterval(800)
        self.save_timer.timeout.connect(self.save_position)
        self.restore_page = int(pos.get("page", 0))
        QTimer.singleShot(0, self.first_layout)

    def title(self):
        return os.path.basename(self.path)

    # ------------------------------------------------------------- barre
    def build_toolbar(self):
        bar = self.bar = QFrame(self)
        bar.setObjectName("pdfbar")
        bar.setStyleSheet(
            f"QFrame#pdfbar {{ background: {BG}; border: 1px solid {BG_HL}; border-radius: 8px; }}"
            f"QFrame#pdfbar QLabel {{ color: {COMMENT}; background: transparent; }}"
            f"QFrame#pdfbar QToolButton {{ padding: 3px; border-radius: 5px; }}"
            f"QFrame#pdfbar QLineEdit {{ padding: 2px 5px; border-radius: 4px; }}")
        h = QHBoxLayout(bar)
        h.setContentsMargins(6, 3, 6, 3)
        h.setSpacing(2)

        def tool(name, tip, fn, col=FG, check=False):
            b = QToolButton()
            b.setIcon(icon(name, col, size=16))
            b.setToolTip(tip)
            b.setCheckable(check)
            b.clicked.connect(fn)
            h.addWidget(b)
            return b
        self.page_in = QLineEdit()
        self.page_in.setFixedWidth(38)
        self.page_in.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.page_in.returnPressed.connect(lambda: self.go_page(int(self.page_in.text() or 1) - 1)
                                           if self.page_in.text().isdigit() else None)
        h.addWidget(self.page_in)
        self.page_lbl = QLabel(f"/ {self.count}")
        h.addWidget(self.page_lbl)
        h.addSpacing(6)
        tool("min", "Zoom arrière (Ctrl+−)", lambda: self.zoom_step(-1))
        self.zoom_lbl = QLabel("100 %")
        self.zoom_lbl.setFixedWidth(46)
        self.zoom_lbl.setAlignment(Qt.AlignmentFlag.AlignCenter)
        h.addWidget(self.zoom_lbl)
        tool("new_plus", "Zoom avant (Ctrl++)", lambda: self.zoom_step(1))
        self.b_fit = tool("fit_width", "Ajuster à la largeur (Ctrl+0)", lambda: self.set_fit(True), BLUE, True)
        self.b_page = tool("fit_page", "Page entière (une page à la fois)", self.toggle_fit_page, BLUE, True)
        self.b_dark = tool("moon", "Mode sombre pour le PDF", self.toggle_dark, YELLOW, True)
        self.b_dark.setChecked(self.dark)
        self.search_in = QLineEdit()
        self.search_in.setPlaceholderText("Rechercher…")
        self.search_in.setFixedWidth(150)
        self.search_in.returnPressed.connect(lambda: self.find(self.search_in.text(), backwards=bool(
            QApplication.keyboardModifiers() & Qt.KeyboardModifier.ShiftModifier)))
        self.search_in.hide()
        self.match_lbl = QLabel("")
        self.match_lbl.hide()
        h.addWidget(self.search_in)
        h.addWidget(self.match_lbl)
        self.b_search = tool("search", "Rechercher (Ctrl+F)", self.show_search, FG)
        tool("external", "Ouvrir dans l'application PDF du système",
             lambda: QDesktopServices.openUrl(QUrl.fromLocalFile(self.path)), COMMENT)
        bar.adjustSize()
        self.bar_timer = QTimer(self)
        self.bar_timer.setSingleShot(True)
        self.bar_timer.setInterval(2200)
        self.bar_timer.timeout.connect(self.hide_toolbar)
        self.place_toolbar()
        self.bar.show()
        self.bar_timer.start()

    def place_toolbar(self):
        self.bar.adjustSize()
        sb = self.area.verticalScrollBar()
        right = sb.width() if sb.isVisible() else 0
        self.bar.move(max(6, self.width() - self.bar.width() - 10 - right), 8)
        self.bar.raise_()

    def poke_toolbar(self):
        if not self.bar.isVisible():
            self.bar.show()
            self.place_toolbar()
        self.bar_timer.start()

    def hide_toolbar(self):
        busy = self.bar.underMouse() or self.page_in.hasFocus() or \
            (self.search_in.isVisible() and (self.search_in.hasFocus() or self.search_in.text()))
        if busy:
            self.bar_timer.start()
        else:
            self.bar.hide()

    def eventFilter(self, obj, ev):
        if ev.type() == ev.Type.MouseMove:
            self.poke_toolbar()
        elif ev.type() == ev.Type.Resize:
            QTimer.singleShot(0, self.relayout)
        return False

    def resizeEvent(self, e):
        super().resizeEvent(e)
        self.place_toolbar()

    # ------------------------------------------------------------ mise en page
    def first_layout(self):
        self.relayout()
        if 0 < self.restore_page < self.count:
            self.go_page(self.restore_page)
        self.update_labels()

    def view_width(self):
        return max(self.area.viewport().width(), 100)

    def relayout(self, keep=None):
        """Recalcule la position des pages ; garde le même endroit du document en vue."""
        vbar = self.area.verticalScrollBar()
        if keep is None and self.offsets:
            top = vbar.value()
            i = self.page_index_at_y(top)
            keep = (i, (top - self.offsets[i]) / max(self.page_h(i), 1))
        if self.fit_page:
            vh = max(self.area.viewport().height() - 2 * MARGIN, 100)
            self.scale = min((self.view_width() - 2 * MARGIN) / self.widest,
                             vh / max(h for w, h in self.sizes))
        elif self.fit:
            self.scale = (self.view_width() - 2 * MARGIN) / self.widest
        self.scale = max(0.2, min(self.scale, 6.0))
        y = MARGIN
        self.offsets = []
        for w, h in self.sizes:
            self.offsets.append(y)
            y += int(h * self.scale) + GAP
        width = max(int(self.widest * self.scale) + 2 * MARGIN, self.view_width())
        self.canvas.resize(width, y - GAP + MARGIN)
        if keep:
            i, frac = keep
            vbar.setValue(int(self.offsets[i] + frac * self.page_h(i)))
        self.update_labels()
        self.canvas.update()

    def page_h(self, i):
        return int(self.sizes[i][1] * self.scale)

    def page_rect(self, i):
        w = int(self.sizes[i][0] * self.scale)
        x = max(MARGIN, (self.canvas.width() - w) // 2)
        return QRect(x, self.offsets[i], w, self.page_h(i))

    def page_index_at_y(self, y):
        lo = 0
        for i, off in enumerate(self.offsets):
            if off <= y:
                lo = i
            else:
                break
        return lo

    def visible_pages(self, rect):
        if not self.offsets:
            return []
        first = self.page_index_at_y(rect.top())
        out = []
        for i in range(first, self.count):
            if self.offsets[i] > rect.bottom():
                break
            out.append(i)
        return out

    def current_page(self):
        if not self.offsets:
            return 0
        vbar = self.area.verticalScrollBar()
        return self.page_index_at_y(vbar.value() + self.area.viewport().height() // 3)

    def go_page(self, i):
        if not self.offsets:
            return
        i = max(0, min(i, self.count - 1))
        self.area.verticalScrollBar().setValue(self.offsets[i] - MARGIN)
        self.update_labels()

    def scrolled(self, _v):
        self.update_labels()
        self.poke_toolbar()
        self.save_timer.start()

    def update_labels(self):
        if not self.page_in.hasFocus():
            self.page_in.setText(str(self.current_page() + 1))
        self.zoom_lbl.setText(f"{round(self.scale * 100 / 1.333)} %")   # 100 % = taille réelle à l'écran
        self.b_fit.setChecked(self.fit and not self.fit_page)
        self.b_page.setChecked(self.fit_page)

    # ----------------------------------------------------------------- zoom
    def zoom_step(self, d, anchor=None):
        cur = self.scale / 1.333
        if d > 0:
            nxt = next((z for z in ZOOMS if z > cur + 0.01), ZOOMS[-1])
        else:
            nxt = next((z for z in reversed(ZOOMS) if z < cur - 0.01), ZOOMS[0])
        self.set_scale(nxt * 1.333, anchor)

    def set_scale(self, scale, anchor=None):
        vbar, hbar = self.area.verticalScrollBar(), self.area.horizontalScrollBar()
        ax = anchor.x() if anchor else self.canvas.width() / 2
        ay = anchor.y() if anchor else vbar.value() + self.area.viewport().height() / 2
        ratio = scale / self.scale
        view_y = ay - vbar.value()
        view_x = ax - hbar.value()
        self.fit = self.fit_page = False
        self.scale = scale
        self.relayout(keep=(0, 0))
        vbar.setValue(int(ay * ratio - view_y))
        hbar.setValue(int(ax * ratio - view_x))
        self.save_timer.start()

    def set_fit(self, on=True):
        self.fit, self.fit_page = on, False
        self.relayout()
        self.save_timer.start()

    def toggle_fit_page(self):
        self.fit_page = not self.fit_page
        if not self.fit_page:
            self.fit = True
        self.relayout()
        self.go_page(self.current_page())

    def toggle_dark(self):
        self.dark = self.b_dark.isChecked()
        _settings().setValue("pdf_dark", self.dark)
        self.cache.clear()
        self.canvas.update()

    # ---------------------------------------------------------------- rendu
    def cached(self, i):
        c = self.cache.get(i)
        if c is None:
            return None
        scale, img = c
        if abs(scale - self.scale) > 1e-3:
            self.queue([i])   # ancienne échelle : affichée en attendant la nouvelle
        return img

    def queue(self, pages):
        for i in pages:
            if i not in self.todo:
                self.todo.append(i)
        if not self.render_timer.isActive():
            self.render_timer.start(0)

    def render_some(self):
        """Rend une page à la fois, pour que l'interface reste fluide."""
        if not self.todo:
            return
        i = self.todo.pop(0)
        c = self.cache.get(i)
        if c is None or abs(c[0] - self.scale) > 1e-3:
            dpr = self.devicePixelRatioF()
            bmp = self.doc[i].render(scale=self.scale * dpr, rev_byteorder=True, may_draw_forms=True)
            img = QImage(bmp.buffer, bmp.width, bmp.height, bmp.stride, QImage.Format.Format_RGB888).copy()
            if self.dark:   # couleurs inversées, puis fond ramené à celui du thème
                img.invertPixels()
                qp = QPainter(img)
                qp.setCompositionMode(QPainter.CompositionMode.CompositionMode_Lighten)
                qp.fillRect(img.rect(), QColor("#1a1b26" if not LIGHT else BG))
                qp.end()
            img.setDevicePixelRatio(dpr)
            self.cache[i] = (self.scale, img)
            if len(self.cache) > CACHE_MAX:   # on oublie les pages les plus éloignées
                cur = self.current_page()
                far = max(self.cache, key=lambda p: abs(p - cur))
                self.cache.pop(far, None)
            self.canvas.update(self.page_rect(i))
        if self.todo:
            self.render_timer.start(0)

    # --------------------------------------------------- coordonnées et texte
    def page_at(self, pos):
        """(page, x, y) en points PDF sous un point de l'écran, ou None."""
        if not self.offsets:
            return None
        i = self.page_index_at_y(int(pos.y()))
        r = self.page_rect(i)
        if not r.contains(pos.toPoint()):
            return None
        x = (pos.x() - r.x()) / self.scale
        y = self.sizes[i][1] - (pos.y() - r.y()) / self.scale
        return i, x, y

    def to_screen(self, i, box):
        left, bottom, right, top = box
        r = self.page_rect(i)
        h = self.sizes[i][1]
        return QRectF(r.x() + left * self.scale, r.y() + (h - top) * self.scale,
                      (right - left) * self.scale, (top - bottom) * self.scale)

    def textpage(self, i):
        tp = self.textpages.get(i)
        if tp is None:
            tp = self.textpages[i] = self.doc[i].get_textpage()
        return tp

    def char_at(self, i, x, y, near=False):
        tol = 4 if near else 12
        return self.textpage(i).get_index(x, y, tol, tol)

    def word_bounds(self, i, idx):
        tp = self.textpage(i)
        n = tp.count_chars()
        text = tp.get_text_range(0, n)
        isw = lambda c: c.isalnum() or c in "_-'’"
        a = b = min(idx, len(text) - 1)
        if a < 0 or not isw(text[a]):
            return idx, idx + 1
        while a > 0 and isw(text[a - 1]):
            a -= 1
        while b < len(text) and isw(text[b]):
            b += 1
        return a, b

    def text_rects(self, i, start, end):
        tp = self.textpage(i)
        n = tp.count_rects(start, max(end - start, 0))
        return [tp.get_rect(k) for k in range(n)]

    def selected_text(self):
        sel = self.canvas.sel
        if not sel:
            return ""
        i, a, b = sel
        return self.textpage(i).get_text_range(a, b - a).replace("\r\n", "\n").strip("\x00")

    def copy(self):
        text = self.selected_text()
        if text:
            QApplication.clipboard().setText(text)
            self.win.statusBar().showMessage("Texte copié.", 2000)

    # ------------------------------------------------------------------ liens
    def page_links(self, i):
        """Liens de la page : [(rectangle PDF, ("url", adresse) ou ("page", numéro))]."""
        if i in self.links:
            return self.links[i]
        out = []
        page = self.doc[i]
        try:   # liens posés par l'auteur du PDF
            pos = ctypes.c_int(0)
            link = praw.FPDF_LINK()
            while praw.FPDFLink_Enumerate(page.raw, ctypes.byref(pos), ctypes.byref(link)):
                rect = praw.FS_RECTF()
                if not praw.FPDFLink_GetAnnotRect(link, ctypes.byref(rect)):
                    continue
                box = (min(rect.left, rect.right), min(rect.bottom, rect.top),
                       max(rect.left, rect.right), max(rect.bottom, rect.top))
                target = None
                action = praw.FPDFLink_GetAction(link)
                if action and praw.FPDFAction_GetType(action) == praw.PDFACTION_URI:
                    n = praw.FPDFAction_GetURIPath(self.doc.raw, action, None, 0)
                    buf = ctypes.create_string_buffer(n)
                    praw.FPDFAction_GetURIPath(self.doc.raw, action, buf, n)
                    target = ("url", buf.value.decode("utf-8", "replace"))
                else:
                    dest = praw.FPDFLink_GetDest(self.doc.raw, link)
                    if not dest and action:
                        dest = praw.FPDFAction_GetDest(self.doc.raw, action)
                    if dest:
                        target = ("page", praw.FPDFDest_GetDestPageIndex(self.doc.raw, dest))
                if target:
                    out.append((box, target))
        except Exception:
            pass
        try:   # adresses écrites en clair dans le texte
            weblinks = praw.FPDFLink_LoadWebLinks(self.textpage(i).raw)
            for k in range(praw.FPDFLink_CountWebLinks(weblinks)):
                n = praw.FPDFLink_GetURL(weblinks, k, None, 0)
                buf = (ctypes.c_ushort * n)()
                praw.FPDFLink_GetURL(weblinks, k, buf, n)
                url = bytes(buf).decode("utf-16-le").rstrip("\x00")
                for j in range(praw.FPDFLink_CountRects(weblinks, k)):
                    l, t, r, b = (ctypes.c_double() for _ in range(4))
                    praw.FPDFLink_GetRect(weblinks, k, j, ctypes.byref(l), ctypes.byref(t),
                                          ctypes.byref(r), ctypes.byref(b))
                    out.append(((l.value, b.value, r.value, t.value), ("url", url)))
            praw.FPDFLink_CloseWebLinks(weblinks)
        except Exception:
            pass
        self.links[i] = out
        return out

    def link_at(self, i, x, y):
        for (l, b, r, t), target in self.page_links(i):
            if l <= x <= r and b <= y <= t:
                return target
        return None

    def follow(self, target):
        kind, value = target
        if kind == "page":
            self.go_page(value)
        elif value:
            if hasattr(self.win, "open_url") and value.startswith(("http://", "https://")):
                self.win.open_url(value)
            else:
                QDesktopServices.openUrl(QUrl(value))

    # -------------------------------------------------------------- recherche
    def show_search(self):
        self.poke_toolbar()
        self.search_in.show()
        self.match_lbl.show()
        self.b_search.hide()
        self.place_toolbar()
        self.search_in.setFocus()
        self.search_in.selectAll()

    def hide_search(self):
        self.search_in.hide()
        self.match_lbl.hide()
        self.b_search.show()
        self.matches, self.match_pos, self.last_search = [], -1, ""
        self.place_toolbar()
        self.canvas.update()
        self.canvas.setFocus()

    def find(self, text, backwards=False):
        text = text.strip()
        if not text:
            return
        if text != self.last_search:
            self.last_search = text
            self.matches, self.match_pos = [], -1
            for i in range(self.count):
                tp = self.textpage(i)
                searcher = tp.search(text, match_case=False)
                while True:
                    hit = searcher.get_next()
                    if not hit:
                        break
                    start, n = hit
                    for box in self.text_rects(i, start, start + n):
                        self.matches.append((i, box))
                        break   # un rectangle par résultat suffit pour le repérer
            # premier résultat à partir de la page courante
            cur = self.current_page()
            self.match_pos = next((k for k, (pg, _) in enumerate(self.matches) if pg >= cur), 0) - 1
        if not self.matches:
            self.match_lbl.setText("0")
            self.match_lbl.setStyleSheet("color: #f7768e;")
            self.canvas.update()
            return
        self.match_pos = (self.match_pos + (-1 if backwards else 1)) % len(self.matches)
        self.match_lbl.setStyleSheet("")
        self.match_lbl.setText(f"{self.match_pos + 1}/{len(self.matches)}")
        i, box = self.matches[self.match_pos]
        r = self.to_screen(i, box)
        self.area.ensureVisible(int(r.center().x()), int(r.center().y()), 60, self.area.viewport().height() // 3)
        self.canvas.update()

    # ---------------------------------------------------------------- mémoire
    def save_position(self):
        allv = load_positions()
        allv[self.path] = {"page": self.current_page(), "fit": self.fit, "scale": round(self.scale, 3)}
        if len(allv) > 200:
            for k in list(allv)[:len(allv) - 200]:
                allv.pop(k, None)
        _settings().setValue("pdf_positions", json.dumps(allv))

    def close_doc(self):
        self.save_position()
        try:
            self.textpages.clear()
            self.doc.close()
        except Exception:
            pass
