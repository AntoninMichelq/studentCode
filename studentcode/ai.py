"""Aide par IA de StudentCode.

Plusieurs types de connexion, à choisir et à authentifier dans « Connexions IA » :
  - une clé d'API (OpenAI, Anthropic, Google Gemini, Mistral) : la discussion se fait
    dans le panneau d'aide, avec le fichier courant comme contexte ;
  - un modèle local (Ollama) ou tout service compatible OpenAI ;
  - le site web de l'assistant, avec ton compte habituel (sans clé).
Les clés sont enregistrées : dans le trousseau du système s'il y en a un, sinon dans
un fichier privé lisible par toi seul. On ne les ressaisit donc pas à chaque lancement.
"""
import json
import os
import threading
import urllib.error
import urllib.request
import uuid

from PyQt6.QtCore import QEvent, QObject, QSettings, Qt, QTimer, QUrl, pyqtSignal
from PyQt6.QtWidgets import (QApplication, QCheckBox, QComboBox, QDialog, QFormLayout, QHBoxLayout,
                             QLabel, QLineEdit, QListWidget, QListWidgetItem, QMessageBox,
                             QPlainTextEdit, QPushButton, QTextBrowser, QToolButton, QVBoxLayout,
                             QWidget)

from icons import icon
from remote import pw_available, pw_del, pw_get, pw_set
from theme import BG, BG_HL, BLUE, COMMENT, FG, GREEN, MAGENTA, RED, YELLOW

KINDS = {  # type de connexion -> (libellé, besoin d'une clé, URL de base modifiable)
    "openai": ("OpenAI (clé d'API)", True, False),
    "anthropic": ("Anthropic Claude (clé d'API)", True, False),
    "gemini": ("Google Gemini (clé d'API)", True, False),
    "mistral": ("Mistral (clé d'API)", True, False),
    "ollama": ("Ollama (modèle local, sans clé)", False, True),
    "custom": ("Service compatible OpenAI (URL au choix)", False, True),
    "web": ("Compte de l'assistant (connexion habituelle, sans clé)", False, False),
}
BASES = {"openai": "https://api.openai.com/v1", "mistral": "https://api.mistral.ai/v1",
         "anthropic": "https://api.anthropic.com/v1",
         "gemini": "https://generativelanguage.googleapis.com/v1beta",
         "ollama": "http://localhost:11434/v1"}
KEY_HELP = {"openai": "platform.openai.com → API keys", "anthropic": "console.anthropic.com → API keys",
            "gemini": "aistudio.google.com → Get API key", "mistral": "console.mistral.ai → API keys"}
WEB_SITES = ["ChatGPT", "Claude", "Gemini", "Mistral (Le Chat)"]
PREFER = {"gemini": ("flash",), "mistral": ("mistral-small-latest", "mistral-large-latest"),
          "openai": ("mini",), "anthropic": ("sonnet",)}


class ApiError(Exception):
    pass


def _request(url, headers, body=None, timeout=120):
    data = json.dumps(body).encode("utf-8") if body is not None else None
    req = urllib.request.Request(url, data=data, method="POST" if data else "GET",
                                 headers={"Content-Type": "application/json", **headers})
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            return json.loads(r.read().decode("utf-8"))
    except urllib.error.HTTPError as e:
        try:
            detail = json.loads(e.read().decode("utf-8", "replace"))
            err = detail.get("error", detail)
            msg = err.get("message") if isinstance(err, dict) else str(err)
        except Exception:
            msg = ""
        hint = {401: "clé d'API refusée", 403: "accès refusé pour cette clé",
                404: "modèle ou adresse introuvable", 429: "quota ou limite de débit atteint",
                402: "crédit insuffisant sur le compte"}.get(e.code, "erreur du service")
        raise ApiError(f"{hint} (HTTP {e.code})" + (f" : {msg}" if msg else ""))
    except urllib.error.URLError as e:
        raise ApiError(f"service injoignable ({e.reason})")
    except TimeoutError:
        raise ApiError("le service met trop de temps à répondre")
    except ValueError:
        raise ApiError("réponse illisible du service")


def base_url(conn):
    return (conn.get("base") or BASES.get(conn["kind"], "")).rstrip("/")


def list_models(conn, key):
    kind = conn["kind"]
    if kind == "anthropic":
        d = _request(BASES["anthropic"] + "/models?limit=100",
                     {"x-api-key": key, "anthropic-version": "2023-06-01"}, timeout=30)
        return [m["id"] for m in d.get("data", [])]
    if kind == "gemini":
        d = _request(BASES["gemini"] + "/models?pageSize=200", {"x-goog-api-key": key}, timeout=30)
        return [m["name"].split("/", 1)[-1] for m in d.get("models", [])
                if "generateContent" in m.get("supportedGenerationMethods", [])]
    d = _request(base_url(conn) + "/models", {"Authorization": "Bearer " + key} if key else {}, timeout=30)
    ids = sorted((m["id"] for m in d.get("data", [])), reverse=True)
    if kind == "openai":
        chat = [i for i in ids if i.startswith(("gpt", "chatgpt", "o1", "o3", "o4")) and
                not any(x in i for x in ("audio", "realtime", "tts", "transcribe", "image", "embedding", "search"))]
        ids = chat or ids
    if kind == "mistral":
        ids = [i for i in ids if "embed" not in i and "moderation" not in i and "ocr" not in i]
    return ids


def pick_model(kind, models):
    for want in PREFER.get(kind, ()):
        for m in models:
            if want in m:
                return m
    return models[0] if models else ""


def chat(conn, key, system, messages):
    """Envoie la discussion et renvoie le texte de la réponse."""
    kind, model = conn["kind"], conn.get("model", "")
    if not model:
        raise ApiError("aucun modèle choisi pour cette connexion (Connexions IA → Charger les modèles)")
    if kind == "anthropic":
        d = _request(BASES["anthropic"] + "/messages",
                     {"x-api-key": key, "anthropic-version": "2023-06-01"},
                     {"model": model, "max_tokens": 4096, "system": system, "messages": messages})
        return "".join(b.get("text", "") for b in d.get("content", []) if b.get("type") == "text")
    if kind == "gemini":
        contents = [{"role": "model" if m["role"] == "assistant" else "user", "parts": [{"text": m["content"]}]}
                    for m in messages]
        d = _request(f"{BASES['gemini']}/models/{model}:generateContent", {"x-goog-api-key": key},
                     {"system_instruction": {"parts": [{"text": system}]}, "contents": contents})
        cands = d.get("candidates") or []
        if not cands:
            raise ApiError("le service n'a pas renvoyé de réponse (" +
                           str(d.get("promptFeedback", {}).get("blockReason", "raison inconnue")) + ")")
        return "".join(p.get("text", "") for p in cands[0].get("content", {}).get("parts", []))
    d = _request(base_url(conn) + "/chat/completions", {"Authorization": "Bearer " + key} if key else {},
                 {"model": model, "messages": [{"role": "system", "content": system}] + messages})
    content = d["choices"][0]["message"]["content"]
    if isinstance(content, list):
        content = "".join(p.get("text", "") for p in content if isinstance(p, dict))
    return content or ""


# ------------------------------------------------------------ connexions
def default_conns():
    """Par défaut : se connecter avec son compte habituel, sans clé."""
    return [{"id": "web-" + s.split()[0].lower(), "name": s.split(" (")[0] + " — mon compte", "kind": "web", "site": s}
            for s in WEB_SITES]


def load_conns():
    try:
        v = json.loads(QSettings("StudentCode", "StudentCode").value("ai_conns", "") or "null")
    except ValueError:
        v = None
    return v if isinstance(v, list) and v else default_conns()


def save_conns(conns):
    QSettings("StudentCode", "StudentCode").setValue("ai_conns", json.dumps(conns))


KEY_FILE = os.path.join(os.path.expanduser("~"), ".config", "StudentCode", "ai_keys.json")
_session_keys = {}


def _file_keys():
    try:
        with open(KEY_FILE, encoding="utf-8") as fh:
            v = json.load(fh)
        return v if isinstance(v, dict) else {}
    except (OSError, ValueError):
        return {}


def _write_file_keys(keys):
    os.makedirs(os.path.dirname(KEY_FILE), exist_ok=True)
    fd = os.open(KEY_FILE, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)  # lisible par toi seul
    with os.fdopen(fd, "w", encoding="utf-8") as fh:
        json.dump(keys, fh)
    os.chmod(KEY_FILE, 0o600)


def key_store():
    """Où sont gardées les clés : « trousseau » ou « fichier »."""
    return "trousseau" if pw_available() else "fichier"


def get_key(conn):
    cid = conn["id"]
    return _session_keys.get(cid) or pw_get("ai-" + cid) or _file_keys().get(cid, "")


def set_key(conn, key):
    """Enregistre la clé durablement (trousseau, sinon fichier privé)."""
    cid = conn["id"]
    _session_keys[cid] = key
    if pw_available() and pw_set("ai-" + cid, key) and pw_get("ai-" + cid) == key:
        keys = _file_keys()
        if keys.pop(cid, None) is not None:
            _write_file_keys(keys)
        return
    keys = _file_keys()
    keys[cid] = key
    _write_file_keys(keys)


def del_key(conn):
    cid = conn["id"]
    _session_keys.pop(cid, None)
    pw_del("ai-" + cid)
    keys = _file_keys()
    if keys.pop(cid, None) is not None:
        _write_file_keys(keys)


class Worker(QObject):
    """Exécute un appel réseau hors de l'interface et renvoie (jeton, résultat, erreur)."""
    done = pyqtSignal(object, object, object)

    def run(self, token, fn):
        def work():
            try:
                res, err = fn(), None
            except ApiError as e:
                res, err = None, str(e)
            except Exception as e:
                res, err = None, f"erreur inattendue : {e}"
            try:
                self.done.emit(token, res, err)
            except RuntimeError:
                pass
        threading.Thread(target=work, daemon=True).start()


class ConnDialog(QDialog):
    """« Connexions IA » : la liste des connexions et leur authentification."""

    def __init__(self, parent):
        super().__init__(parent)
        self.setWindowTitle("Connexions IA")
        self.resize(760, 470)
        self.conns = load_conns()
        self.removed = []
        self.cur = None
        self.worker = Worker()
        self.worker.done.connect(self.net_done)
        self.list = QListWidget()
        self.list.setMinimumWidth(240)
        self.list.currentRowChanged.connect(self.show_conn)
        add = QPushButton("  Ajouter")
        add.setIcon(icon("new", FG))
        add.clicked.connect(self.add)
        rm = QPushButton("  Supprimer")
        rm.setIcon(icon("trash", RED))
        rm.clicked.connect(self.remove)
        left = QVBoxLayout()
        left.addWidget(self.list, 1)
        row = QHBoxLayout()
        row.addWidget(add)
        row.addWidget(rm)
        left.addLayout(row)
        self.kind = QComboBox()
        for k, (label, _, _) in KINDS.items():
            self.kind.addItem(label, k)
        self.kind.currentIndexChanged.connect(self.kind_changed)
        self.name = QLineEdit()
        self.site = QComboBox()
        self.site.addItems(WEB_SITES)
        self.key = QLineEdit()
        self.key.setEchoMode(QLineEdit.EchoMode.Password)
        self.key_hint = QLabel()
        self.key_hint.setStyleSheet(f"color: {COMMENT};")
        self.key_hint.setWordWrap(True)
        self.base = QLineEdit()
        self.model = QComboBox()
        self.model.setEditable(True)
        self.b_models = QPushButton("Charger les modèles")
        self.b_models.clicked.connect(self.fetch_models)
        mrow = self.mrow = QHBoxLayout()
        mrow.addWidget(self.model, 1)
        mrow.addWidget(self.b_models)
        self.form = QFormLayout()
        self.form.addRow("Type de connexion", self.kind)
        self.form.addRow("Nom", self.name)
        self.form.addRow("Assistant", self.site)
        self.form.addRow("Clé d'API", self.key)
        self.form.addRow("", self.key_hint)
        self.form.addRow("Adresse du service", self.base)
        self.form.addRow("Modèle", mrow)
        self.status = QLabel()
        self.status.setWordWrap(True)
        self.b_test = QPushButton("Tester la connexion")
        self.b_test.clicked.connect(self.test)
        ok = QPushButton("Enregistrer")
        ok.setObjectName("primary")
        ok.clicked.connect(self.accept_all)
        cancel = QPushButton("Annuler")
        cancel.clicked.connect(self.reject)
        bottom = QHBoxLayout()
        bottom.addWidget(self.b_test)
        bottom.addStretch(1)
        bottom.addWidget(cancel)
        bottom.addWidget(ok)
        right = QVBoxLayout()
        right.addLayout(self.form)
        right.addWidget(self.status)
        right.addStretch(1)
        right.addLayout(bottom)
        lay = QHBoxLayout(self)
        lay.addLayout(left)
        lay.addLayout(right, 1)
        self.refill(0)
        if not self.conns:
            self.add()

    def refill(self, row):
        self.list.blockSignals(True)
        self.list.clear()
        for c in self.conns:
            it = QListWidgetItem(icon("globe" if c["kind"] == "web" else "key", BLUE if c["kind"] == "web" else YELLOW),
                                 c["name"])
            self.list.addItem(it)
        self.list.blockSignals(False)
        self.cur = None
        self.list.setCurrentRow(min(row, len(self.conns) - 1))

    def store(self):
        """Reporte le formulaire dans la connexion affichée."""
        c = self.cur
        if c is None:
            return
        c["kind"] = self.kind.currentData()
        c["name"] = self.name.text().strip() or KINDS[c["kind"]][0]
        c["site"] = self.site.currentText()
        c["base"] = self.base.text().strip()
        c["model"] = self.model.currentText().strip()
        if self.key.text():
            c["_key"] = self.key.text()
        it = self.list.item(self.conns.index(c))
        if it:
            it.setText(c["name"])

    def show_conn(self, row):
        self.store()
        if not (0 <= row < len(self.conns)):
            self.cur = None
            return
        c = self.conns[row]
        self.cur = None
        self.kind.setCurrentIndex(max(self.kind.findData(c["kind"]), 0))
        self.name.setText(c["name"])
        self.site.setCurrentText(c.get("site", WEB_SITES[0]))
        self.key.setText(c.get("_key", ""))
        self.base.setText(c.get("base", ""))
        self.model.clear()
        self.model.addItems(c.get("_models", []))
        self.model.setCurrentText(c.get("model", ""))
        self.cur = c
        self.kind_changed()
        self.status.setText("")

    def kind_changed(self, *_):
        kind = self.kind.currentData()
        _, need_key, has_base = KINDS[kind]
        web = kind == "web"
        auto = {"openai": "ChatGPT (OpenAI)", "anthropic": "Claude", "gemini": "Gemini", "mistral": "Mistral",
                "ollama": "Ollama (local)", "custom": "Service personnalisé", "web": "Site web"}
        if self.cur is not None and self.name.text().strip() in list(auto.values()) + ["", "Nouvelle connexion"]:
            self.name.setText(auto[kind])
        saved = bool(self.cur and (get_key(self.cur) or self.cur.get("_key")))
        for w, on in ((self.site, web), (self.key, not web), (self.key_hint, not web),
                      (self.base, has_base), (self.model, not web), (self.b_models, not web)):
            w.setVisible(on)
            lab = self.form.labelForField(w)
            if lab:
                lab.setVisible(on)
        lab = self.form.labelForField(self.mrow)
        if lab:
            lab.setVisible(not web)
        self.key.setPlaceholderText("clé enregistrée — laisse vide pour la garder" if saved
                                    else ("colle ta clé ici" if need_key else "facultative"))
        hint = KEY_HELP.get(kind, "")
        store = "Elle est enregistrée dans le trousseau du système." if key_store() == "trousseau" else \
            "Elle est enregistrée dans un fichier privé, lisible par toi seul."
        self.key_hint.setText((f"Où la trouver : {hint}. " if hint else "") + store)
        self.base.setPlaceholderText(BASES.get(kind, "https://…/v1"))
        self.b_test.setVisible(not web)

    def add(self):
        self.store()
        c = {"id": uuid.uuid4().hex[:10], "name": "Gemini", "kind": "gemini", "model": ""}
        self.conns.append(c)
        self.refill(len(self.conns) - 1)

    def remove(self):
        row = self.list.currentRow()
        if 0 <= row < len(self.conns):
            c = self.conns.pop(row)
            self.removed.append(c)
            self.refill(max(row - 1, 0))

    def current_key(self):
        return self.key.text() or (get_key(self.cur) if self.cur else "")

    def busy(self, on, text=""):
        self.b_models.setEnabled(not on)
        self.b_test.setEnabled(not on)
        if on:
            self.status.setStyleSheet(f"color: {YELLOW};")
            self.status.setText(text)

    def check_ready(self):
        self.store()
        c = self.cur
        if c is None:
            return None
        if KINDS[c["kind"]][1] and not self.current_key():
            self.status.setStyleSheet(f"color: {RED};")
            self.status.setText("Colle d'abord ta clé d'API.")
            return None
        return c

    def fetch_models(self):
        c = self.check_ready()
        if c:
            key = self.current_key()
            self.busy(True, "Chargement des modèles…")
            self.worker.run(("models", c["id"]), lambda: list_models(dict(c), key))

    def test(self):
        c = self.check_ready()
        if c:
            key = self.current_key()
            self.busy(True, "Test en cours…")
            self.worker.run(("test", c["id"]), lambda: chat(
                dict(c), key, "Réponds en un seul mot.", [{"role": "user", "content": "Dis « prêt »."}]))

    def net_done(self, token, res, err):
        what, cid = token
        self.busy(False)
        c = next((x for x in self.conns if x["id"] == cid), None)
        if c is None or c is not self.cur:
            return
        if err:
            self.status.setStyleSheet(f"color: {RED};")
            self.status.setText("Échec : " + err)
            return
        self.status.setStyleSheet(f"color: {GREEN};")
        if what == "models":
            c["_models"] = res
            keep = self.model.currentText().strip()
            self.model.clear()
            self.model.addItems(res)
            self.model.setCurrentText(keep if keep in res else pick_model(c["kind"], res))
            self.status.setText(f"{len(res)} modèles disponibles." if res else "Aucun modèle trouvé.")
        else:
            self.status.setText("Connexion valide : le service a répondu.")

    def accept_all(self):
        self.store()
        for c in self.removed:
            del_key(c)
        for c in self.conns:
            key = c.pop("_key", None)
            c.pop("_models", None)
            if key:
                set_key(c, key)
        save_conns(self.conns)
        self.accept()


class Input(QPlainTextEdit):
    submitted = pyqtSignal()

    def keyPressEvent(self, e):
        if e.key() in (Qt.Key.Key_Return, Qt.Key.Key_Enter) and \
                not e.modifiers() & Qt.KeyboardModifier.ShiftModifier:
            self.submitted.emit()
        else:
            super().keyPressEvent(e)


class AiPanel(QWidget):
    """Panneau d'aide contextuel (à droite) : discussion avec l'IA choisie, le fichier
    courant servant de contexte."""

    def __init__(self, win, tasks, preprompt, sites=None):
        super().__init__()
        self.win = win
        self.sites = sites or {}        # nom de l'assistant -> adresse de sa page de discussion
        self.web = None                 # page de l'assistant affichée dans le panneau (connexion « compte »)
        self.web_site = None
        self.tasks = tasks              # clé -> (libellé, consigne)
        self.default_preprompt = preprompt
        self.history = []               # messages envoyés au service
        self.shown = []                 # (rôle, texte affiché)
        self.sent_context = None        # dernier contexte joint (pour ne pas le renvoyer à l'identique)
        self.pending = 0
        self.worker = Worker()
        self.worker.done.connect(self.answered)
        self.setMinimumWidth(300)
        self.combo = QComboBox()
        self.combo.setToolTip("Connexion IA utilisée")
        self.combo.currentIndexChanged.connect(self.conn_changed)

        def tool(name, tip, fn, col=FG):
            b = QToolButton()
            b.setIcon(icon(name, col))
            b.setToolTip(tip)
            b.clicked.connect(fn)
            return b
        top = QHBoxLayout()
        top.setContentsMargins(8, 6, 6, 4)
        top.setSpacing(2)
        spark = QLabel()
        from icons import pixmap
        spark.setPixmap(pixmap("spark", MAGENTA, 16))
        top.addWidget(spark)
        top.addWidget(self.combo, 1)
        top.addWidget(tool("key", "Connexions IA (ajouter une clé, choisir un modèle)", self.manage, YELLOW))
        top.addWidget(tool("trash", "Nouvelle discussion", self.clear, COMMENT))
        top.addWidget(tool("close", "Fermer le panneau", self.hide, COMMENT))
        self.view = QTextBrowser()
        self.view.setOpenExternalLinks(True)
        self.view.document().setDefaultStyleSheet(
            f"pre, code {{ font-family: '{win.code_font.family()}', monospace; background: {BG_HL}; }}"
            f"a {{ color: {BLUE}; }}")
        self.view.setStyleSheet(f"QTextBrowser {{ background: {BG}; border: none; padding: 6px; }}")
        from PyQt6.QtWidgets import QGridLayout
        quick = QGridLayout()
        quick.setContentsMargins(8, 4, 8, 0)
        quick.setSpacing(4)
        for i, (key, (label, _)) in enumerate(tasks.items()):
            b = QPushButton(label)
            b.setStyleSheet("QPushButton { padding: 5px 8px; }")
            b.clicked.connect(lambda _=False, k=key: self.send(task=k))
            quick.addWidget(b, i // 2, i % 2)
        self.quick_w = QWidget()
        self.quick_w.setLayout(quick)
        self.ctx = QCheckBox()
        self.ctx.setChecked(True)
        self.ctx.setStyleSheet(f"color: {COMMENT}; padding: 2px 8px;")
        self.input = Input()
        self.input.setPlaceholderText("Pose ta question… (Entrée pour envoyer, Maj+Entrée pour une nouvelle ligne)")
        self.input.setFixedHeight(64)
        self.input.submitted.connect(lambda: self.send())
        self.b_send = QPushButton("Envoyer")
        self.b_send.setObjectName("primary")
        self.b_send.clicked.connect(lambda: self.send())
        self.b_copy = QPushButton("Copier la réponse")
        self.b_copy.clicked.connect(self.copy_last)
        pre = QPushButton("Pré-prompt…")
        pre.clicked.connect(win.edit_preprompt)
        bot = QHBoxLayout()
        bot.setContentsMargins(8, 2, 8, 8)
        bot.addWidget(pre)
        bot.addWidget(self.b_copy)
        bot.addStretch(1)
        bot.addWidget(self.b_send)
        lay = QVBoxLayout(self)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.setSpacing(0)
        lay.addLayout(top)
        from PyQt6.QtWidgets import QStackedWidget
        self.stack = QStackedWidget()
        self.stack.addWidget(self.view)
        self.web_box = QWidget()
        self.web_lay = QVBoxLayout(self.web_box)
        self.web_lay.setContentsMargins(0, 0, 0, 0)
        self.web_lay.setSpacing(0)
        self.web_hint = QLabel("Pas encore connecté ? Connecte-toi une fois : la session est conservée.")
        self.web_hint.setWordWrap(True)
        self.web_hint.setStyleSheet(f"color: {COMMENT}; padding: 3px 8px;")
        self.b_login = QPushButton("  Se connecter")
        self.b_login.setIcon(icon("key", YELLOW))
        self.b_login.setToolTip("Ouvre la page de connexion dans un grand onglet ; "
                                "une fois connecté, la discussion revient ici toute seule.")
        self.b_login.clicked.connect(lambda: self.open_login())
        hint_row = QHBoxLayout()
        hint_row.setContentsMargins(0, 0, 6, 2)
        hint_row.addWidget(self.web_hint, 1)
        hint_row.addWidget(self.b_login)
        self.web_lay.addLayout(hint_row)
        self.login_tab = None
        self.stack.addWidget(self.web_box)
        lay.addWidget(self.stack, 1)
        lay.addWidget(self.quick_w)
        lay.addWidget(self.ctx)
        box = QVBoxLayout()
        box.setContentsMargins(8, 2, 8, 4)
        box.addWidget(self.input)
        lay.addLayout(box)
        lay.addLayout(bot)
        self.b_setup = QPushButton("  Ajouter une connexion IA")
        self.b_setup.setIcon(icon("key", YELLOW))
        self.b_setup.setObjectName("primary")
        self.b_setup.clicked.connect(self.manage)
        lay.insertWidget(2, self.b_setup)
        self.reload()
        self.render()

    # -------------------------------------------------------- connexions
    def reload(self):
        conns = load_conns()
        want = QSettings("StudentCode", "StudentCode").value("ai_conn", "")
        self.combo.blockSignals(True)
        self.combo.clear()
        for c in conns:
            self.combo.addItem(icon("globe" if c["kind"] == "web" else "key",
                                    BLUE if c["kind"] == "web" else YELLOW), c["name"], c["id"])
        self.combo.setCurrentIndex(max(self.combo.findData(want), 0))
        self.combo.blockSignals(False)
        self.conn_changed()

    def conn(self):
        cid = self.combo.currentData()
        return next((c for c in load_conns() if c["id"] == cid), None)

    def conn_changed(self, *_):
        c = self.conn()
        if c:
            QSettings("StudentCode", "StudentCode").setValue("ai_conn", c["id"])
        self.render()

    # ---------------------------------------- connexion « compte » : la page dans le panneau
    def web_ok(self):
        try:
            from PyQt6.QtWebEngineWidgets import QWebEngineView  # noqa: F401
            return True
        except Exception:
            return False

    def show_web(self, c):
        """Affiche la discussion de l'assistant dans le panneau, avec le compte de l'utilisateur."""
        from PyQt6.QtWebEngineCore import QWebEnginePage
        from PyQt6.QtWebEngineWidgets import QWebEngineView
        site = c.get("site", WEB_SITES[0])
        if self.web is None:
            panel = self

            class AiWeb(QWebEngineView):
                def createWindow(self, _type):
                    # fenêtre de connexion (Google, Apple, lien magique…) : dans un grand onglet
                    tab = panel.open_login(url=None)
                    return tab.view if tab is not None else None
            self.web = AiWeb()
            self.web.setPage(QWebEnginePage(self.win.web_profile(), self.web))
            self.web_lay.addWidget(self.web, 1)
        if self.web_site != site:
            self.web_site = site
            self.web_loaded = False
            self.web.loadFinished.connect(self._web_ready)
            self.web.setUrl(QUrl(self.sites.get(site) or next(iter(self.sites.values()))))
        self.stack.setCurrentWidget(self.web_box)

    def _web_ready(self, _ok):
        self.web_loaded = True

    # ------------------------------------------- connexion au compte dans un onglet
    @staticmethod
    def is_login_url(url):
        u = url.toString().lower()
        return any(w in u for w in ("login", "signin", "sign-in", "sign_in", "auth", "accounts.google",
                                    "appleid", "oauth", "magic-link", "verify", "sso", "challenge"))

    def open_login(self, url="site"):
        """Ouvre la connexion au compte dans un onglet de la zone centrale. Dès que la page revient
        sur la discussion (connexion réussie), l'onglet se ferme et le panneau se recharge connecté."""
        win = self.win
        if url == "site":
            url = self.sites.get(self.web_site or WEB_SITES[0]) or next(iter(self.sites.values()))
        if self.login_tab is not None and self.login_tab in win.all_tabs() and url:
            win.pane_of(self.login_tab).setCurrentWidget(self.login_tab)
            self.login_tab.load(url)
            return self.login_tab
        tab = win.open_url(url, pane=None)
        if tab is None:
            return None
        self.login_tab = tab
        tab.seen_login = False
        home = QUrl(self.sites.get(self.web_site or WEB_SITES[0]) or "").host()

        def changed(u, t=tab):
            if self.is_login_url(u):
                t.seen_login = True
            elif t.seen_login and u.host() == home:   # revenu sur le site après la connexion
                QTimer.singleShot(800, lambda: self.login_done(t))
        tab.view.urlChanged.connect(changed)
        tab.view.page().windowCloseRequested.connect(lambda t=tab: self.login_done(t))
        win.statusBar().showMessage("Connecte-toi dans cet onglet : l'aide IA revient toute seule après.", 9000)
        return tab

    def login_done(self, tab):
        win = self.win
        if tab in win.all_tabs():
            win.close_widget(tab)
        if self.login_tab is tab:
            self.login_tab = None
        if self.web is not None:
            self.web.reload()
        win.ai_panel.show() if not self.isVisible() else None
        win.ai_btn.setChecked(True)
        win.statusBar().showMessage("Connecté : l'aide IA est prête.", 5000)

    def paste_in_web(self, prompt):
        QApplication.clipboard().setText(prompt)
        from PyQt6.QtWebEngineCore import QWebEnginePage

        def paste():
            if self.web is not None:
                self.web.setFocus()
                self.web.page().triggerAction(QWebEnginePage.WebAction.Paste)
        # si la page charge encore, on attend qu'elle soit prête
        QTimer.singleShot(200 if getattr(self, "web_loaded", False) else 2500, paste)
        self.win.statusBar().showMessage(
            "Message collé dans la discussion — s'il n'apparaît pas, clique dans la zone de saisie puis Ctrl+V.", 9000)

    def manage(self):
        if ConnDialog(self).exec():
            self.reload()

    # --------------------------------------------------------- affichage
    def hideEvent(self, e):
        super().hideEvent(e)
        if self.web is not None:   # panneau fermé : la page de l'assistant est mise en veille
            if not hasattr(self, "sleep_timer"):
                self.sleep_timer = QTimer(self)
                self.sleep_timer.setSingleShot(True)
                self.sleep_timer.timeout.connect(self.sleep_web)
            self.sleep_timer.start(60000)

    def sleep_web(self):
        if self.web is not None and not self.isVisible():
            from lazy import discard
            discard(self.web.page())

    def showEvent(self, e):
        super().showEvent(e)
        if getattr(self, "sleep_timer", None):
            self.sleep_timer.stop()
        if self.web is not None:
            from lazy import wake
            wake(self.web.page())
        self.render()   # la page de l'assistant n'est chargée qu'à l'ouverture du panneau
        self.refresh_context_label()
        self.input.setFocus()

    def refresh_context_label(self):
        label, _ = self.win.ai_context()
        self.ctx.setText("Joindre : " + label if label else "Aucun fichier de code ouvert")
        self.ctx.setEnabled(bool(label))

    def event(self, e):
        if e.type() == QEvent.Type.WindowActivate:
            self.refresh_context_label()
        return super().event(e)

    def render(self):
        c = self.conn()
        self.b_setup.setVisible(c is None)
        web = bool(c and c["kind"] == "web" and self.web_ok() and self.isVisible())
        self.b_copy.setVisible(not (c and c["kind"] == "web"))
        if web:
            return self.show_web(c)
        self.stack.setCurrentWidget(self.view)
        if c is None:
            self.view.setMarkdown(
                "### Aide par IA\n\nPour discuter avec une IA directement ici, il faut une connexion :\n\n"
                "1. Clique sur **Ajouter une connexion IA**.\n"
                "2. Choisis le service et colle ta **clé d'API** (elle est enregistrée, tu ne la ressaisis pas).\n"
                "3. **Charger les modèles**, choisis-en un, puis **Enregistrer**.\n\n"
                "Où obtenir une clé :\n\n"
                "- **Gemini** : aistudio.google.com → « Get API key » (offre gratuite limitée).\n"
                "- **Mistral** : console.mistral.ai → « API keys » (offre gratuite limitée).\n"
                "- **ChatGPT (OpenAI)** : platform.openai.com → « API keys » (payant à l'usage).\n"
                "- **Claude** : console.anthropic.com → « API keys » (payant à l'usage).\n"
                "- **Ollama** : un modèle qui tourne sur ton PC, sans clé ni compte.\n\n"
                "Avec les offres gratuites, tes messages peuvent servir à améliorer les modèles du service.")
            return
        if not self.shown:
            if c and c["kind"] == "web":
                how = (f"Connexion « {c['name']} » : ta question est préparée avec ton code, puis collée "
                       "dans la discussion de l'assistant. Tu envoies toi-même.")
            elif c and KINDS[c["kind"]][1] and not get_key(c):
                how = (f"La connexion « {c['name']} » n'a pas encore de clé : clique sur la clé jaune "
                       "en haut du panneau pour l'ajouter.")
            else:
                how = "Choisis une action ou pose ta question : la réponse s'affiche ici."
            self.view.setMarkdown(
                "### Aide\n\n" + how + "\n\nLe fichier ouvert (ou ta sélection) est joint comme contexte ; "
                "pour une erreur, la sortie du programme l'est aussi.\n\nUne IA peut se tromper : "
                "vérifie ce qu'elle affirme en exécutant ton code.")
            return
        parts = []
        for role, text in self.shown:
            who = {"user": "**Toi**", "assistant": "**Assistant**", "error": "**Erreur**",
                   "wait": "**Assistant**"}[role]
            parts.append(who + "\n\n" + text)
        self.view.setMarkdown("\n\n---\n\n".join(parts))
        sb = self.view.verticalScrollBar()
        sb.setValue(sb.maximum())

    def clear(self):
        self.history, self.shown, self.sent_context = [], [], None
        self.pending += 1  # une réponse encore en route sera ignorée
        self.set_busy(False)
        self.render()

    def copy_last(self):
        last = next((t for r, t in reversed(self.shown) if r == "assistant"), "")
        if last:
            QApplication.clipboard().setText(last)
            self.win.statusBar().showMessage("Réponse copiée.", 2500)

    def set_busy(self, on):
        self.b_send.setEnabled(not on)
        self.quick_w.setEnabled(not on)

    # ------------------------------------------------------------- envoi
    def send(self, task=None):
        c = self.conn()
        if c is None:
            return self.manage()
        if not self.b_send.isEnabled():
            return
        ask = self.tasks[task][1] if task else self.input.toPlainText().strip()
        if not ask:
            return
        label, block = self.win.ai_context() if self.ctx.isChecked() else ("", "")
        output = self.win.ai_output(force=task == "debug")
        system = self.win.ai_preprompt()
        if c["kind"] == "web":
            prompt = "\n".join(x for x in (system, "", "Ma demande : " + ask, "", block, output) if x is not None)
            if self.web_ok():
                self.show_web(c)
                self.paste_in_web(prompt.rstrip() + "\n")
            else:   # pas de moteur web : on ouvre le site dans le navigateur
                self.win.open_ai_site(c.get("site", WEB_SITES[0]), prompt.rstrip() + "\n")
            if not task:
                self.input.clear()
            return
        key = get_key(c)
        if KINDS[c["kind"]][1] and not key:
            self.shown.append(("error", f"La connexion « {c['name']} » n'a pas de clé d'API. "
                                        "Clique sur la clé jaune en haut du panneau pour l'ajouter."))
            return self.render()
        content, note = ask, ""
        context = "\n".join(x for x in (block, output) if x)
        if context and context != self.sent_context:  # joint seulement s'il a changé
            content = ask + "\n\n" + context
            self.sent_context = context
            note = f"\n\n*({label} joint" + (", avec la sortie du programme" if output else "") + ")*"
        self.history.append({"role": "user", "content": content})
        self.shown.append(("user", (self.tasks[task][0] if task else ask) + note))
        self.shown.append(("wait", "*réflexion en cours…*"))
        if not task:
            self.input.clear()
        self.set_busy(True)
        self.render()
        self.pending += 1
        msgs = [dict(m) for m in self.history]
        self.worker.run(self.pending, lambda: chat(dict(c), key, system, msgs))

    def answered(self, token, text, err):
        if token != self.pending:
            return
        self.set_busy(False)
        if self.shown and self.shown[-1][0] == "wait":
            self.shown.pop()
        if err:
            if self.history and self.history[-1]["role"] == "user":
                self.history.pop()      # la question pourra être reposée
                self.sent_context = None
            self.shown.append(("error", err))
        else:
            text = (text or "").strip() or "*(réponse vide)*"
            self.history.append({"role": "assistant", "content": text})
            self.shown.append(("assistant", text))
        self.render()
