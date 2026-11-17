"""Terminal intégré de StudentCode : un vrai terminal (couleurs, invite du shell,
vim, nano, complétion par Tab), pour le shell local comme pour la console SSH.

L'émulation est faite par la bibliothèque pyte ; ce fichier dessine l'écran et
traduit le clavier.
"""
import fcntl
import os
import pty
import signal
import struct
import termios

from PyQt6.QtCore import QEvent, QSocketNotifier, Qt, QTimer, pyqtSignal
from PyQt6.QtGui import QColor, QFont, QFontMetricsF, QPainter
from PyQt6.QtWidgets import QApplication, QLabel, QVBoxLayout, QWidget

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

    def __init__(self, font, placeholder=""):
        super().__init__()
        self.setFocusPolicy(Qt.FocusPolicy.StrongFocus)
        self.setAttribute(Qt.WidgetAttribute.WA_InputMethodEnabled, True)
        self.setAttribute(Qt.WidgetAttribute.WA_OpaquePaintEvent, True)
        self.font_ = QFont(font)
        fm = QFontMetricsF(self.font_)
        self.cw = max(fm.horizontalAdvance("M"), 1.0)
        self.ch = max(fm.height(), 1.0)
        self.asc = fm.ascent()
        self.bold_ = QFont(self.font_)
        self.bold_.setBold(True)
        self.cols, self.rows = 80, 24
        self.screen = Screen(self.cols, self.rows, history=6000, ratio=0.5)
        self.screen.write_process_input = lambda data: self.send.emit(data.encode("utf-8"))
        self.stream = pyte.ByteStream(self.screen)
        self.colors = {}
        self.sel_a = self.sel_b = None
        self.placeholder = placeholder
        self.started = False
        self.repaint_timer = QTimer(self)
        self.repaint_timer.setSingleShot(True)
        self.repaint_timer.setInterval(12)
        self.repaint_timer.timeout.connect(self.update)
        self.blink = QTimer(self)
        self.blink.setInterval(550)
        self.blink.timeout.connect(self.toggle_cursor)
        self.cursor_on = True

    # ------------------------------------------------------------ contenu
    def feed(self, data):
        self.started = True
        # revient en bas si on avait remonté dans l'historique
        while self.screen.history.position < self.screen.history.size:
            self.screen.next_page()
        try:
            self.stream.feed(data)
        except Exception:  # une séquence exotique ne doit jamais casser le terminal
            pass
        if not self.repaint_timer.isActive():
            self.repaint_timer.start()

    def say(self, text, color=COMMENT):
        """Message de StudentCode affiché dans le terminal (une ligne colorée)."""
        self.feed((rgb_seq(color) + text.replace("\n", "\r\n") + "\x1b[0m\r\n").encode("utf-8"))

    def clear(self):
        self.screen.reset()
        self.update()

    def text(self):
        return "\n".join(self.screen.display)

    def send_text(self, text):
        self.send.emit(text.encode("utf-8"))

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

    def in_sel(self, x, y, rng):
        (ax, ay), (bx, by) = rng
        return (ay, ax) <= (y, x) < (by, bx)

    def paintEvent(self, _e):
        p = QPainter(self)
        p.fillRect(self.rect(), QColor(BG))
        if not self.started and self.placeholder:
            p.setPen(QColor(COMMENT))
            p.setFont(self.font_)
            p.drawText(self.rect().adjusted(10, 8, -10, -8), Qt.TextFlag.TextWordWrap, self.placeholder)
            p.end()
            return
        cw, ch = self.cw, self.ch
        rng = self.sel_range()
        buf = self.screen.buffer
        for y in range(self.screen.lines):
            line = buf[y]
            py = y * ch
            for x in range(self.screen.columns):
                c = line[x]
                data = c.data
                fg, bg = c.fg, c.bg
                fgc = self.color(fg, FG, c.bold)
                bgc = self.color(bg, BG) if bg != "default" else None
                if c.reverse:
                    fgc, bgc = (bgc or QColor(BG)), fgc
                if rng and self.in_sel(x, y, rng):
                    bgc = QColor(SEL)
                px = x * cw
                if bgc is not None:
                    p.fillRect(int(px), int(py), int(cw + 1), int(ch + 1), bgc)
                if data and data != " ":
                    p.setFont(self.bold_ if c.bold else self.font_)
                    p.setPen(fgc)
                    p.drawText(int(px), int(py + self.asc), data)
                if c.underscore:
                    p.setPen(fgc)
                    p.drawLine(int(px), int(py + ch - 2), int(px + cw), int(py + ch - 2))
        cur = self.screen.cursor
        if not cur.hidden and self.screen.history.position == self.screen.history.size:
            r = (int(cur.x * cw), int(cur.y * ch), int(cw), int(ch))
            if self.hasFocus():
                if self.cursor_on:
                    p.fillRect(*r, QColor(FG))
                    ch_under = buf[cur.y][cur.x].data
                    if ch_under.strip():
                        p.setPen(QColor(BG))
                        p.setFont(self.font_)
                        p.drawText(r[0], int(cur.y * ch + self.asc), ch_under)
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

    def resizeEvent(self, e):
        cols = max(int(self.width() / self.cw), 20)
        rows = max(int(self.height() / self.ch), 4)
        if (cols, rows) != (self.cols, self.rows):
            self.cols, self.rows = cols, rows
            self.screen.resize(rows, cols)
            self.resized.emit(cols, rows)
        self.update()

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
            self.screen.prev_page() if k == K.Key_PageUp else self.screen.next_page()
            return self.update()
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
        self.send.emit(out.encode("utf-8"))

    def inputMethodEvent(self, e):
        if e.commitString():
            self.send.emit(e.commitString().encode("utf-8"))

    # -------------------------------------------------- souris / presse-papiers
    def cell(self, pos):
        return (min(max(int(pos.x() / self.cw), 0), self.screen.columns),
                min(max(int(pos.y() / self.ch), 0), self.screen.lines - 1))

    def mousePressEvent(self, e):
        self.setFocus()
        if e.button() == Qt.MouseButton.LeftButton:
            self.sel_a = self.sel_b = self.cell(e.position())
            self.update()
        elif e.button() == Qt.MouseButton.MiddleButton:
            self.paste(QApplication.clipboard().Mode.Selection)

    def mouseMoveEvent(self, e):
        if e.buttons() & Qt.MouseButton.LeftButton and self.sel_a is not None:
            self.sel_b = self.cell(e.position())
            self.update()

    def mouseReleaseEvent(self, e):
        text = self.selected_text()
        if text and QApplication.clipboard().supportsSelection():
            QApplication.clipboard().setText(text, QApplication.clipboard().Mode.Selection)

    def wheelEvent(self, e):
        self.screen.prev_page() if e.angleDelta().y() > 0 else self.screen.next_page()
        self.update()

    def selected_text(self):
        rng = self.sel_range()
        if not rng:
            return ""
        (ax, ay), (bx, by) = rng
        lines = self.screen.display
        out = []
        for y in range(ay, by + 1):
            row = lines[y]
            x0 = ax if y == ay else 0
            x1 = bx if y == by else len(row)
            out.append(row[x0:x1].rstrip())
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


class LocalTerminal(QWidget):
    """Terminal local : lance le shell de l'utilisateur dans un pseudo-terminal."""

    def __init__(self, win):
        super().__init__()
        self.win = win
        self.pid = self.fd = self.notifier = None
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
        lay.addWidget(self.term)
        self.setFocusProxy(self.term)

    def showEvent(self, e):
        super().showEvent(e)
        if self.term is not None and self.pid is None:
            QTimer.singleShot(0, self.start)

    def start(self):
        if self.pid is not None or self.term is None:
            return
        shell = os.environ.get("SHELL") or "/bin/bash"
        cwd = self.win.root if os.path.isdir(self.win.root) else os.path.expanduser("~")
        pid, fd = pty.fork()
        if pid == 0:  # processus fils : devient le shell
            try:
                os.chdir(cwd)
            except OSError:
                pass
            env = dict(os.environ, TERM="xterm-256color", COLORTERM="truecolor")
            env.pop("QT_LOGGING_RULES", None)
            try:
                os.execvpe(shell, [shell, "-i"], env)
            finally:
                os._exit(127)
        self.pid, self.fd = pid, fd
        self.set_size(self.term.cols, self.term.rows)
        self.notifier = QSocketNotifier(fd, QSocketNotifier.Type.Read, self)
        self.notifier.activated.connect(self.read)

    def read(self, *_):
        try:
            data = os.read(self.fd, 65536)
        except OSError:
            data = b""
        if not data:
            return self.ended()
        self.term.feed(data)

    def ended(self):
        if self.notifier is not None:
            self.notifier.setEnabled(False)
            self.notifier.deleteLater()
            self.notifier = None
        try:
            os.close(self.fd)
        except (OSError, TypeError):
            pass
        try:
            os.waitpid(self.pid, os.WNOHANG)
        except (OSError, TypeError):
            pass
        self.pid = self.fd = None
        self.term.say("\n[shell terminé — appuie sur Entrée pour en relancer un]")

    def write(self, data):
        if self.fd is None:
            if data in (b"\r", b"\n"):
                self.term.clear()
                self.start()
            return
        try:
            os.write(self.fd, data)
        except OSError:
            self.ended()

    def set_size(self, cols, rows):
        if self.fd is not None:
            try:
                fcntl.ioctl(self.fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))
            except OSError:
                pass

    def cd(self, path):
        """Place le shell dans `path` (quand on change de dossier de projet)."""
        if self.fd is not None:
            import shlex
            self.write((" cd " + shlex.quote(path) + "\r").encode("utf-8"))

    def shutdown(self):
        if self.pid is not None:
            try:
                os.kill(self.pid, signal.SIGHUP)
            except OSError:
                pass
