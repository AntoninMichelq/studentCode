"""Terminal intégré de StudentCode : un vrai terminal (couleurs, invite du shell,
vim, nano, complétion par Tab), pour le shell local comme pour la console SSH.

L'émulation est faite par la bibliothèque pyte ; ce fichier dessine l'écran et
traduit le clavier.
"""
import os
import shutil
import signal
import struct
import sys
import threading

WINDOWS = sys.platform == "win32"
if not WINDOWS:
    import fcntl
    import pty
    import termios

from PyQt6.QtCore import QEvent, Qt, QTimer, pyqtSignal
from PyQt6.QtGui import QColor, QFont, QFontMetricsF, QPainter
from PyQt6.QtWidgets import QApplication, QLabel, QMenu, QScrollBar, QVBoxLayout, QWidget

from theme import BG, BG_HL, BLUE, COMMENT, CYAN, FG, GREEN, GUTTER, MAGENTA, RED, SEL, YELLOW

try:
    import pyte
    HAS_TERM = True
except Exception:
    pyte = None
    HAS_TERM = False

BASE = {"black": GUTTER, "red": RED, "green": GREEN, "brown": YELLOW, "blue": BLUE,
        "magenta": MAGENTA, "cyan": CYAN, "white": FG}
DECCKM, BRACKETED = 1 << 5, 2004 << 5
K = Qt.Key
KEYS = {K.Key_Return: "\r", K.Key_Enter: "\r", K.Key_Backspace: "\x7f", K.Key_Tab: "\t",
        K.Key_Backtab: "\x1b[Z", K.Key_Escape: "\x1b", K.Key_Delete: "\x1b[3~",
        K.Key_Insert: "\x1b[2~", K.Key_PageUp: "\x1b[5~", K.Key_PageDown: "\x1b[6~",
        K.Key_F1: "\x1bOP", K.Key_F2: "\x1bOQ", K.Key_F3: "\x1bOR", K.Key_F4: "\x1bOS",
        K.Key_F5: "\x1b[15~", K.Key_F6: "\x1b[17~", K.Key_F7: "\x1b[18~", K.Key_F8: "\x1b[19~",
        K.Key_F9: "\x1b[20~", K.Key_F10: "\x1b[21~", K.Key_F11: "\x1b[23~", K.Key_F12: "\x1b[24~"}
ARROWS = {K.Key_Up: "A", K.Key_Down: "B", K.Key_Right: "C", K.Key_Left: "D",
          K.Key_Home: "H", K.Key_End: "F"}


if HAS_TERM:
    class Screen(pyte.HistoryScreen):
        """Ajoute l'écran alternatif (vim, nano, less, htop le demandent) : à la sortie
        du programme, on retrouve l'écran d'avant."""
        _saved = None

        def set_mode(self, *modes, **kwargs):
            if kwargs.get("private") and any(m in (1049, 1047, 47) for m in modes) and self._saved is None:
                self._saved = ({y: dict(line) for y, line in self.buffer.items()},
                               self.cursor.x, self.cursor.y)
                self.erase_in_display(2)
                self.cursor_position()
            super().set_mode(*modes, **kwargs)

        def reset_mode(self, *modes, **kwargs):
            if kwargs.get("private") and any(m in (1049, 1047, 47) for m in modes) and self._saved is not None:
                lines, x, y = self._saved
                self._saved = None
                self.buffer.clear()
                for row, line in lines.items():
                    self.buffer[row].update(line)
                self.cursor.x, self.cursor.y = x, min(y, self.lines - 1)
                self.dirty.update(range(self.lines))
            super().reset_mode(*modes, **kwargs)


def rgb_seq(hexcolor):
    c = QColor(hexcolor)
    return "\x1b[38;2;%d;%d;%dm" % (c.red(), c.green(), c.blue())


class TermWidget(QWidget):
    """Écran de terminal. `send` (signal) porte ce que tape l'utilisateur ; `feed()` reçoit
    ce que le programme affiche ; `resized` annonce une nouvelle taille (colonnes, lignes)."""
    send = pyqtSignal(bytes)
    resized = pyqtSignal(int, int)
    kind = "xterm-256color"
    PAD = 6           # marge à gauche et en haut, en pixels
    WHEEL_LINES = 3   # lignes par cran de molette
    show_bar = False  # barre de défilement (option « Barres de défilement »)

    def __init__(self, font, placeholder=""):
        super().__init__()
        self.setFocusPolicy(Qt.FocusPolicy.StrongFocus)
        self.setAttribute(Qt.WidgetAttribute.WA_InputMethodEnabled, True)
        self.setAttribute(Qt.WidgetAttribute.WA_OpaquePaintEvent, True)
        self.setCursor(Qt.CursorShape.IBeamCursor)
        self.font_ = QFont(font)
        fm = QFontMetricsF(self.font_)
        self.cw = max(fm.horizontalAdvance("M"), 1.0)
        self.ch = max(fm.height(), 1.0)
        self.asc = fm.ascent()
        self.bold_ = QFont(self.font_)
        self.bold_.setBold(True)
        self.cols, self.rows = 80, 24
        self.screen = Screen(self.cols, self.rows, history=8000, ratio=0.5)
        self.screen.write_process_input = lambda data: self.send.emit(data.encode("utf-8"))
        self.stream = pyte.ByteStream(self.screen)
        self.colors = {}
        self.sel_a = self.sel_b = None
        self.selecting = False
        self.clicks = 0
        self.wheel_acc = 0
        self.placeholder = placeholder
        self.started = False
        self.bar = QScrollBar(Qt.Orientation.Vertical, self)
        self.bar.setCursor(Qt.CursorShape.ArrowCursor)
        self.bar.valueChanged.connect(self.bar_moved)
        self.bar_sync = False
        self.repaint_timer = QTimer(self)
        self.repaint_timer.setSingleShot(True)
        self.repaint_timer.setInterval(12)
        self.repaint_timer.timeout.connect(self.refresh)
        self.resize_timer = QTimer(self)   # on attend la fin du redimensionnement avant de prévenir le shell
        self.resize_timer.setSingleShot(True)
        self.resize_timer.setInterval(110)
        self.resize_timer.timeout.connect(self.apply_size)
        self.click_timer = QTimer(self)
        self.click_timer.setSingleShot(True)
        self.click_timer.setInterval(QApplication.doubleClickInterval())
        self.click_timer.timeout.connect(lambda: setattr(self, "clicks", 0))
        self.blink = QTimer(self)
        self.blink.setInterval(550)
        self.blink.timeout.connect(self.toggle_cursor)
        self.cursor_on = True

    # ------------------------------------------------------------ contenu
    def feed(self, data):
        self.started = True
        self.scroll_bottom()
        if not self.selecting:
            self.sel_a = self.sel_b = None   # le texte bouge : la sélection ne désigne plus rien
        try:
            self.stream.feed(data)
        except Exception:  # une séquence exotique ne doit jamais casser le terminal
            pass
        if not self.repaint_timer.isActive():
            self.repaint_timer.start()

    def refresh(self):
        self.sync_bar()
        self.update()

    def say(self, text, color=COMMENT):
        """Message de StudentCode affiché dans le terminal (une ligne colorée)."""
        self.feed((rgb_seq(color) + text.replace("\n", "\r\n") + "\x1b[0m\r\n").encode("utf-8"))

    def clear(self):
        self.screen.reset()
        self.sel_a = self.sel_b = None
        self.refresh()

    def text(self):
        return "\n".join(self.screen.display)

    def send_text(self, text):
        self.send.emit(text.encode("utf-8"))

    # ------------------------------------------------------- défilement
    def scrolled_back(self):
        return len(self.screen.history.bottom)

    def scroll_lines(self, n):
        """Remonte de n lignes dans l'historique (n < 0 : redescend)."""
        if n == 0 or self.screen._saved is not None:
            return
        ratio = self.screen.history.ratio
        left, page = abs(n), max(self.screen.lines, 1)
        while left > 0:                      # par tranches d'au plus un écran
            step = min(left, page)
            self.screen.history = self.screen.history._replace(ratio=step / page)
            moved = len(self.screen.history.bottom)
            if n > 0:
                self.screen.prev_page()
            else:
                self.screen.next_page()
            left -= step
            if len(self.screen.history.bottom) == moved:
                break                        # début ou fin de l'historique atteint
        self.screen.history = self.screen.history._replace(ratio=ratio)
        self.sel_a = self.sel_b = None
        self.refresh()

    def scroll_bottom(self):
        guard = 0
        while self.screen.history.bottom and guard < 10000:
            self.screen.next_page()
            guard += 1

    def sync_bar(self):
        h = self.screen.history
        top, bottom = len(h.top), len(h.bottom)
        self.bar_sync = True
        self.bar.setRange(0, top + bottom)
        self.bar.setPageStep(max(self.screen.lines, 1))
        self.bar.setValue(top)
        self.bar.setVisible(TermWidget.show_bar and top + bottom > 0 and self.screen._saved is None)
        self.bar_sync = False

    def bar_moved(self, value):
        if not self.bar_sync:
            self.scroll_lines(len(self.screen.history.top) - value)

    def wheelEvent(self, e):
        self.wheel_acc += e.angleDelta().y()
        step = 120 // self.WHEEL_LINES          # un cran de molette = WHEEL_LINES lignes
        n = int(self.wheel_acc / step)
        if not n:
            return
        self.wheel_acc -= n * step
        if self.screen._saved is not None:      # vim, less, nano… : la molette déplace le curseur
            key = ("\x1bO" if DECCKM in self.screen.mode else "\x1b[") + ("A" if n > 0 else "B")
            self.send.emit((key * min(abs(n), 10)).encode())
        else:
            self.scroll_lines(n)

    # ------------------------------------------------------------- dessin
    def color(self, name, default, bold=False):
        if name == "default":
            return QColor(default)
        key = (name, bold)
        c = self.colors.get(key)
        if c is None:
            if name.startswith("bright"):
                c = QColor(BASE.get(name[6:], default)).lighter(118)
            elif name in BASE:
                c = QColor(BASE[name])
            else:
                c = QColor("#" + name) if len(name) == 6 else QColor(default)
                if not c.isValid():
                    c = QColor(default)
            self.colors[key] = c
        return c

    def sel_range(self):
        if self.sel_a is None or self.sel_b is None or self.sel_a == self.sel_b:
            return None
        a, b = sorted([self.sel_a, self.sel_b], key=lambda p: (p[1], p[0]))
        return a, b

    def paintEvent(self, _e):
        p = QPainter(self)
        p.fillRect(self.rect(), QColor(BG))
        if not self.started and self.placeholder:
            p.setPen(QColor(COMMENT))
            p.setFont(self.font_)
            p.drawText(self.rect().adjusted(10, 8, -10, -8), Qt.TextFlag.TextWordWrap, self.placeholder)
            p.end()
            return
        cw, ch, pad = self.cw, self.ch, self.PAD
        rng = self.sel_range()
        selc = QColor(SEL)
        buf = self.screen.buffer
        ncols, nlines = self.screen.columns, self.screen.lines
        for y in range(nlines):
            line = buf[y]
            py = pad + y * ch
            if rng:
                (ax, ay), (bx, by) = rng
                if ay <= y <= by:   # une bande continue par ligne : sélection nette, sans trous
                    x0 = ax if y == ay else 0
                    x1 = bx if y == by else ncols
                    if x1 > x0:
                        p.fillRect(int(pad + x0 * cw), int(py), int((x1 - x0) * cw + 1), int(ch + 1), selc)
            for x in range(ncols):
                c = line[x]
                data = c.data
                fgc = self.color(c.fg, FG, c.bold)
                bgc = self.color(c.bg, BG) if c.bg != "default" else None
                if c.reverse:
                    fgc, bgc = (bgc or QColor(BG)), fgc
                selected = bool(rng) and (rng[0][1], rng[0][0]) <= (y, x) < (rng[1][1], rng[1][0])
                px = pad + x * cw
                if bgc is not None and not selected:
                    p.fillRect(int(px), int(py), int(cw + 1), int(ch + 1), bgc)
                if data and data != " ":
                    p.setFont(self.bold_ if c.bold else self.font_)
                    p.setPen(fgc)
                    p.drawText(int(px), int(py + self.asc), data)
                if c.underscore:
                    p.setPen(fgc)
                    p.drawLine(int(px), int(py + ch - 2), int(px + cw), int(py + ch - 2))
        cur = self.screen.cursor
        if not cur.hidden and not self.scrolled_back() and cur.y < nlines:
            r = (int(pad + cur.x * cw), int(pad + cur.y * ch), int(cw), int(ch))
            if self.hasFocus():
                if self.cursor_on:
                    p.fillRect(*r, QColor(FG))
                    ch_under = buf[cur.y][cur.x].data
                    if ch_under.strip():
                        p.setPen(QColor(BG))
                        p.setFont(self.font_)
                        p.drawText(r[0], int(pad + cur.y * ch + self.asc), ch_under)
            else:
                p.setPen(QColor(COMMENT))
                p.drawRect(r[0], r[1], r[2] - 1, r[3] - 1)
        p.end()

    def toggle_cursor(self):
        self.cursor_on = not self.cursor_on
        self.update()

    def focusInEvent(self, e):
        self.cursor_on = True
        self.blink.start()
        self.update()

    def focusOutEvent(self, e):
        self.blink.stop()
        self.update()

    # ---------------------------------------------------- redimensionnement
    def wanted_size(self):
        w = self.width() - 2 * self.PAD - (self.bar.sizeHint().width() if TermWidget.show_bar else 0)
        h = self.height() - 2 * self.PAD
        return max(int(w / self.cw), 20), max(int(h / self.ch), 4)

    def resizeEvent(self, e):
        bw = self.bar.sizeHint().width()
        self.bar.setGeometry(self.width() - bw, 0, bw, self.height())
        if self.wanted_size() != (self.cols, self.rows):
            self.resize_timer.start()   # appliqué une seule fois, à la fin du geste
        self.update()

    def showEvent(self, e):
        super().showEvent(e)
        self.apply_size(force=True)     # un onglet resté caché n'avait pas encore sa vraie taille

    def apply_size(self, force=False):
        cols, rows = self.wanted_size()
        changed = (cols, rows) != (self.cols, self.rows)
        if changed:
            self.scroll_bottom()
            self.sel_a = self.sel_b = None
            self.cols, self.rows = cols, rows
            self.screen.resize(rows, cols)
        if changed or force:
            self.resized.emit(self.cols, self.rows)
        self.refresh()

    # ------------------------------------------------------------ clavier
    def event(self, e):
        # Ctrl+lettre va au shell (Ctrl+C, Ctrl+R, Ctrl+W…) au lieu des raccourcis de l'application
        if e.type() == QEvent.Type.ShortcutOverride:
            mod = e.modifiers()
            ctrl = mod & Qt.KeyboardModifier.ControlModifier
            other = mod & (Qt.KeyboardModifier.AltModifier | Qt.KeyboardModifier.ShiftModifier)
            if (ctrl and not other and K.Key_A <= e.key() <= K.Key_Z) or e.key() in (K.Key_Escape, K.Key_Tab):
                e.accept()
                return True
        return super().event(e)

    def focusNextPrevChild(self, _next):
        return False  # Tab sert à la complétion du shell

    def keyPressEvent(self, e):
        k, mod = e.key(), e.modifiers()
        ctrl = bool(mod & Qt.KeyboardModifier.ControlModifier)
        shift = bool(mod & Qt.KeyboardModifier.ShiftModifier)
        alt = bool(mod & Qt.KeyboardModifier.AltModifier)
        if ctrl and shift and k == K.Key_C:
            return self.copy()
        if ctrl and shift and k == K.Key_V:
            return self.paste()
        if shift and k in (K.Key_PageUp, K.Key_PageDown):
            page = max(self.screen.lines - 2, 1)
            return self.scroll_lines(page if k == K.Key_PageUp else -page)
        out = None
        if k in ARROWS:
            app = DECCKM in self.screen.mode
            if ctrl:
                out = "\x1b[1;5" + ARROWS[k]
            else:
                out = ("\x1bO" if app else "\x1b[") + ARROWS[k]
        elif k in KEYS:
            out = KEYS[k]
        elif ctrl and not alt and K.Key_A <= k <= K.Key_Z:
            out = chr(k - K.Key_A + 1)
        elif ctrl and k in (K.Key_Space, K.Key_At):
            out = "\x00"
        elif ctrl and k == K.Key_BracketLeft:
            out = "\x1b"
        elif ctrl and k == K.Key_Backslash:
            out = "\x1c"
        elif ctrl and k == K.Key_BracketRight:
            out = "\x1d"
        elif e.text():
            out = e.text()
            if alt and not ctrl:  # Alt+touche = Échap puis la touche
                out = "\x1b" + out
        if out is None:
            return super().keyPressEvent(e)
        self.sel_a = self.sel_b = None
        self.cursor_on = True
        self.scroll_bottom()
        self.send.emit(out.encode("utf-8"))

    def inputMethodEvent(self, e):
        if e.commitString():
            self.send.emit(e.commitString().encode("utf-8"))

    # -------------------------------------------------- souris / presse-papiers
    def cell(self, pos, edge=False):
        """Cellule sous le pointeur ; avec `edge`, la frontière la plus proche (pour sélectionner)."""
        fx = (pos.x() - self.PAD) / self.cw
        x = int(fx + 0.5) if edge else int(fx)
        y = int((pos.y() - self.PAD) / self.ch)
        return (min(max(x, 0), self.screen.columns), min(max(y, 0), self.screen.lines - 1))

    def row_text(self, y):
        line = self.screen.buffer[y]
        return [line[x].data or " " for x in range(self.screen.columns)]

    def word_at(self, x, y):
        row = self.row_text(y)
        x = min(x, len(row) - 1)
        is_word = lambda c: c.isalnum() or c in "_-./~:@+%#="
        if not is_word(row[x]):
            return (x, y), (x + 1, y)
        a = b = x
        while a > 0 and is_word(row[a - 1]):
            a -= 1
        while b < len(row) and is_word(row[b]):
            b += 1
        return (a, y), (b, y)

    def mousePressEvent(self, e):
        self.setFocus()
        if e.button() == Qt.MouseButton.LeftButton:
            self.clicks = self.clicks + 1 if self.click_timer.isActive() else 1
            self.click_timer.start()
            x, y = self.cell(e.position())
            if self.clicks == 2:        # double-clic : le mot
                self.sel_a, self.sel_b = self.word_at(x, y)
                self.selecting = False
                self.selection_done()
            elif self.clicks >= 3:      # triple-clic : la ligne
                self.sel_a, self.sel_b = (0, y), (self.screen.columns, y)
                self.selecting = False
                self.selection_done()
            else:
                self.sel_a = self.sel_b = self.cell(e.position(), edge=True)
                self.selecting = True
            self.update()
        elif e.button() == Qt.MouseButton.MiddleButton:
            self.paste(QApplication.clipboard().Mode.Selection)

    def mouseMoveEvent(self, e):
        if self.selecting and e.buttons() & Qt.MouseButton.LeftButton:
            self.sel_b = self.cell(e.position(), edge=True)
            self.update()

    def mouseReleaseEvent(self, e):
        if e.button() == Qt.MouseButton.LeftButton and self.selecting:
            self.selecting = False
            self.selection_done()

    def selection_done(self):
        text = self.selected_text()
        cb = QApplication.clipboard()
        if text and cb.supportsSelection():
            cb.setText(text, cb.Mode.Selection)

    def contextMenuEvent(self, e):
        m = QMenu(self)
        a_copy = m.addAction("Copier  (Ctrl+Maj+C)")
        a_copy.setEnabled(bool(self.selected_text()))
        a_paste = m.addAction("Coller  (Ctrl+Maj+V)")
        m.addSeparator()
        a_all = m.addAction("Tout sélectionner (écran)")
        a_clear = m.addAction("Effacer l'écran")
        r = m.exec(e.globalPos())
        if r == a_copy:
            self.copy()
        elif r == a_paste:
            self.paste()
        elif r == a_all:
            self.sel_a, self.sel_b = (0, 0), (self.screen.columns, self.screen.lines - 1)
            self.update()
        elif r == a_clear:
            self.send.emit(b"\x0c")   # Ctrl+L : le shell efface et réaffiche son invite

    def selected_text(self):
        rng = self.sel_range()
        if not rng:
            return ""
        (ax, ay), (bx, by) = rng
        out = []
        for y in range(ay, by + 1):
            row = self.row_text(y)
            x0 = ax if y == ay else 0
            x1 = bx if y == by else len(row)
            out.append("".join(row[x0:x1]).rstrip())
        return "\n".join(out)

    def copy(self):
        text = self.selected_text()
        if text:
            QApplication.clipboard().setText(text)

    def paste(self, mode=None):
        cb = QApplication.clipboard()
        text = cb.text(mode) if mode is not None else cb.text()
        if not text:
            return
        text = text.replace("\r\n", "\r").replace("\n", "\r")
        if BRACKETED in self.screen.mode:
            text = "\x1b[200~" + text + "\x1b[201~"
        self.scroll_bottom()
        self.send.emit(text.encode("utf-8"))


class LineTerm(QWidget):
    """Console de secours, ligne par ligne, quand pyte n'est pas installé."""
    send = pyqtSignal(bytes)
    resized = pyqtSignal(int, int)
    cols, rows, kind = 200, 50, "dumb"

    def __init__(self, font, placeholder=""):
        super().__init__()
        import re
        from PyQt6.QtWidgets import QLineEdit, QPlainTextEdit
        self.ansi = re.compile(r"\x1b\][^\x07\x1b]*(\x07|\x1b\\\\)|\x1b\[[0-9;?]*[ -/]*[@-~]|\x1b[=>()][0-9A-B]?|\r")
        self.out = QPlainTextEdit()
        self.out.setReadOnly(True)
        self.out.setFont(font)
        self.inp = QLineEdit()
        self.inp.setFont(font)
        self.inp.setPlaceholderText(placeholder or "commande — ^C pour interrompre")
        self.inp.returnPressed.connect(self.submit)
        lay = QVBoxLayout(self)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.addWidget(self.out)
        lay.addWidget(self.inp)

    def submit(self):
        t = self.inp.text()
        self.inp.clear()
        keys = {"^C": "\x03", "^D": "\x04", "^Z": "\x1a"}
        self.send.emit(keys.get(t.strip().upper(), t + "\n").encode("utf-8"))

    def feed(self, data):
        self.out.moveCursor(self.out.textCursor().MoveOperation.End)
        self.out.insertPlainText(self.ansi.sub("", data.decode("utf-8", "replace")))
        self.out.ensureCursorVisible()

    def say(self, text, color=COMMENT):
        self.feed((text + "\n").encode("utf-8"))

    def clear(self):
        self.out.clear()

    def text(self):
        return self.out.toPlainText()

    def send_text(self, text):
        self.send.emit(text.encode("utf-8"))


def make_term(font, placeholder=""):
    """Un vrai terminal si pyte est là, sinon la console ligne par ligne."""
    return TermWidget(font, placeholder) if HAS_TERM else LineTerm(font, placeholder)


class PosixShell:
    """Shell de l'utilisateur dans un pseudo-terminal (Linux, macOS)."""

    def __init__(self, owner):
        self.owner = owner
        self.pid = self.fd = self.notifier = None

    def start(self, cols, rows, cwd):
        shell = os.environ.get("SHELL") or "/bin/bash"
        pid, fd = pty.fork()
        if pid == 0:  # processus fils : devient le shell
            try:
                os.chdir(cwd)
            except OSError:
                pass
            from sysinfo import child_env
            env = dict(child_env(), TERM="xterm-256color", COLORTERM="truecolor")
            env.pop("QT_LOGGING_RULES", None)
            try:
                os.execvpe(shell, [shell, "-i"], env)
            finally:
                os._exit(127)
        self.pid, self.fd = pid, fd
        self.resize(cols, rows)
        from PyQt6.QtCore import QSocketNotifier
        self.notifier = QSocketNotifier(fd, QSocketNotifier.Type.Read, self.owner)
        self.notifier.activated.connect(self.read)

    def alive(self):
        return self.fd is not None

    def read(self, *_):
        try:
            data = os.read(self.fd, 65536)
        except OSError:
            data = b""
        if not data:
            return self.owner.ended()
        self.owner.term.feed(data)

    def write(self, data):
        os.write(self.fd, data)

    def resize(self, cols, rows):
        if self.fd is not None:
            try:
                fcntl.ioctl(self.fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))
            except OSError:
                pass

    def close(self, kill=False):
        if self.notifier is not None:
            self.notifier.setEnabled(False)
            self.notifier.deleteLater()
            self.notifier = None
        if kill and self.pid is not None:
            try:
                os.kill(self.pid, signal.SIGHUP)
            except OSError:
                pass
        try:
            os.close(self.fd)
        except (OSError, TypeError):
            pass
        try:
            os.waitpid(self.pid, os.WNOHANG)
        except (OSError, TypeError):
            pass
        self.pid = self.fd = None


class WindowsShell:
    """PowerShell (ou cmd) dans un pseudo-terminal Windows (ConPTY, via pywinpty)."""

    def __init__(self, owner):
        self.owner = owner
        self.proc = None

    def start(self, cols, rows, cwd):
        from winpty import PtyProcess
        shell = shutil.which("pwsh") or shutil.which("powershell") or os.environ.get("COMSPEC", "cmd.exe")
        args = [shell, "-NoLogo"] if "powershell" in shell.lower() or "pwsh" in shell.lower() else [shell]
        from sysinfo import child_env
        env = dict(child_env(), TERM="xterm-256color", COLORTERM="truecolor")
        env.pop("QT_LOGGING_RULES", None)
        self.proc = PtyProcess.spawn(args, cwd=cwd, env=env, dimensions=(rows, cols))
        proc = self.proc

        def pump():   # lecture bloquante dans un fil séparé, puis retour dans l'interface par signal
            while True:
                try:
                    text = proc.read(4096)
                except (EOFError, OSError):
                    break
                if text:
                    try:
                        self.owner.data_in.emit(text.encode("utf-8", "replace"))
                    except RuntimeError:
                        return
            try:
                self.owner.shell_gone.emit()
            except RuntimeError:
                pass
        threading.Thread(target=pump, daemon=True).start()

    def alive(self):
        return self.proc is not None

    def write(self, data):
        self.proc.write(data.decode("utf-8", "replace"))

    def resize(self, cols, rows):
        if self.proc is not None:
            try:
                self.proc.setwinsize(rows, cols)
            except Exception:
                pass

    def close(self, kill=False):
        if self.proc is not None and kill:
            try:
                self.proc.terminate(force=True)
            except Exception:
                pass
        self.proc = None


class LocalTerminal(QWidget):
    """Terminal local : le shell de l'utilisateur (bash, zsh… ou PowerShell sous Windows)."""
    data_in = pyqtSignal(bytes)
    shell_gone = pyqtSignal()

    def __init__(self, win):
        super().__init__()
        self.win = win
        self.shell = None
        lay = QVBoxLayout(self)
        lay.setContentsMargins(6, 4, 0, 0)
        if not HAS_TERM:
            self.term = None
            lab = QLabel("Le module du terminal (pyte) n'est pas installé.\n"
                         "Installe « pyte » depuis Bibliothèques, puis relance StudentCode.")
            lab.setStyleSheet(f"color: {COMMENT}; padding: 10px;")
            lay.addWidget(lab)
            lay.addStretch(1)
            return
        self.term = TermWidget(win.code_font)
        self.term.send.connect(self.write)
        self.term.resized.connect(self.set_size)
        self.data_in.connect(self.term.feed)
        self.shell_gone.connect(self.ended)
        lay.addWidget(self.term)
        self.setFocusProxy(self.term)

    @property
    def pid(self):   # compatibilité : « un shell tourne »
        return 1 if self.shell is not None and self.shell.alive() else None

    def showEvent(self, e):
        super().showEvent(e)
        if self.term is not None and self.shell is None:
            QTimer.singleShot(0, self.start)

    def start(self):
        if self.shell is not None or self.term is None:
            return
        cwd = self.win.root if os.path.isdir(self.win.root) else os.path.expanduser("~")
        shell = WindowsShell(self) if WINDOWS else PosixShell(self)
        try:
            shell.start(self.term.cols, self.term.rows, cwd)
        except Exception as e:
            self.term.say(f"Impossible de lancer le shell : {e}")
            return
        self.shell = shell

    def ended(self):
        if self.shell is not None:
            self.shell.close()
        self.shell = None
        self.term.say("\n[shell terminé — appuie sur Entrée pour en relancer un]")

    def write(self, data):
        if self.shell is None:
            if data in (b"\r", b"\n"):
                self.term.clear()
                self.start()
            return
        try:
            self.shell.write(data)
        except OSError:
            self.ended()

    def set_size(self, cols, rows):
        if self.shell is not None:
            self.shell.resize(cols, rows)

    def cd(self, path):
        """Place le shell dans `path` (quand on change de dossier de projet)."""
        if self.shell is not None:
            from sysinfo import quote
            self.write((" cd " + quote(path) + "\r").encode("utf-8"))

    def shutdown(self):
        if self.shell is not None:
            self.shell.close(kill=True)
            self.shell = None
