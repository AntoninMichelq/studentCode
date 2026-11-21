"""VPN de SCode : import de fichiers .ovpn et connexion en un clic.

On passe par NetworkManager (commande nmcli), comme les réglages réseau
d'Ubuntu : pas besoin de droits administrateur pour se connecter, et le VPN
ajouté ici apparaît aussi dans le menu réseau du système.
"""
import ipaddress
import json
import os
import re
import shutil
import tempfile

from PyQt6.QtCore import QProcess, QSettings, Qt, QTimer, QUrl
from PyQt6.QtGui import QDesktopServices
from PyQt6.QtGui import QColor
from PyQt6.QtWidgets import (QCheckBox, QDialog, QDialogButtonBox, QFileDialog, QFormLayout,
                             QHBoxLayout, QLabel, QLineEdit, QMessageBox, QPushButton,
                             QToolButton, QTreeWidget, QTreeWidgetItem, QVBoxLayout, QWidget)

from icons import icon
from remote import pw_available, pw_del, pw_get, pw_set

from theme import BLUE, COMMENT, CYAN, FG, GREEN, RED, YELLOW
SPLIT = re.compile(r"(?<!\\):")
PLUGIN_HINT = ("Le module OpenVPN de NetworkManager n'est pas installé. Dans un terminal :\n\n"
               "    sudo apt install network-manager-openvpn-gnome\n\npuis réessaie.")


def parse_list(text):
    """Sortie de « nmcli -t -f NAME,UUID,TYPE,ACTIVE,STATE connection show »."""
    out = []
    for line in text.splitlines():
        parts = [p.replace("\\:", ":").replace("\\\\", "\\") for p in SPLIT.split(line)]
        if len(parts) >= 4 and parts[2] in ("vpn", "wireguard"):
            state = parts[4] if len(parts) > 4 else ""
            out.append({"name": parts[0], "uuid": parts[1], "type": parts[2],
                        "active": parts[3] == "yes" and state != "activating",
                        "busy": state in ("activating", "deactivating")})
    return out


def nm_error(text):
    """Traduit les erreurs courantes de nmcli."""
    low = text.lower()
    if "plugin" in low and ("openvpn" in low or "vpn" in low) and ("not" in low or "unknown" in low or "missing" in low):
        return PLUGIN_HINT
    if "secrets were required" in low or "no valid secrets" in low or "login failed" in low:
        return "Identifiant ou mot de passe du VPN refusé."
    if "timeout" in low or "timed out" in low:
        return "Le serveur VPN ne répond pas (connexion internet ? serveur accessible d'ici ?)."
    if "not authorized" in low or "insufficient privileges" in low:
        return "Ton compte n'a pas le droit de gérer les connexions réseau."
    if "service" in low and "stopped" in low:
        return "Le VPN s'est arrêté pendant la connexion (identifiants ou configuration refusés)."
    text = text.strip() or "erreur inconnue"
    return text if len(text) < 400 else text[:400] + "…"


def load_opts():
    try:
        v = json.loads(QSettings("StudentCode", "StudentCode").value("vpn_opts", "") or "{}")
    except ValueError:
        v = {}
    return v if isinstance(v, dict) else {}


def vpn_opts(uuid):
    """Réglages d'un VPN : `split` (ne faire passer par le VPN que le réseau du lab),
    `routes` (réseaux ajoutés à la main), `hosts` (machines des labs SSH liés)."""
    o = load_opts().get(uuid, {})
    return {"split": o.get("split", True), "routes": o.get("routes", ""), "hosts": o.get("hosts", [])}


def save_opts(uuid, opts):
    allv = load_opts()
    allv[uuid] = opts
    QSettings("StudentCode", "StudentCode").setValue("vpn_opts", json.dumps(allv))


def clean_routes(text):
    """« 10.22.0.0/16, 10.0.3.7 » -> liste de réseaux valides (IPv4)."""
    out = []
    for part in re.split(r"[,;\s]+", text.strip()):
        if not part:
            continue
        try:
            net = ipaddress.ip_network(part, strict=False)
        except ValueError:
            continue
        if net.version == 4 and str(net) not in out:
            out.append(str(net))
    return out


def route_args(uuid):
    """Arguments nmcli qui appliquent les réglages du VPN."""
    o = vpn_opts(uuid)
    if not o["split"]:
        return ["ipv4.never-default", "no", "ipv6.never-default", "no", "ipv4.routes", ""]
    routes = clean_routes(o["routes"] + " " + " ".join(o["hosts"]))
    return ["ipv4.never-default", "yes", "ipv6.never-default", "yes", "ipv4.routes", ", ".join(routes)]


class CredDialog(QDialog):
    def __init__(self, parent, name, user="", why=""):
        super().__init__(parent)
        self.setWindowTitle("Identifiants du VPN")
        self.setMinimumWidth(420)
        self.user = QLineEdit(user)
        self.pw = QLineEdit()
        self.pw.setEchoMode(QLineEdit.EchoMode.Password)
        self.save = QCheckBox("Enregistrer le mot de passe dans le trousseau du système")
        ok = pw_available()
        self.save.setChecked(ok)
        self.save.setEnabled(ok)
        f = QFormLayout()
        f.addRow("Identifiant", self.user)
        f.addRow("Mot de passe", self.pw)
        bb = QDialogButtonBox(QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel)
        bb.button(QDialogButtonBox.StandardButton.Ok).setText("Valider")
        bb.button(QDialogButtonBox.StandardButton.Cancel).setText("Annuler")
        bb.accepted.connect(self.accept)
        bb.rejected.connect(self.reject)
        lay = QVBoxLayout(self)
        head = QLabel(f"VPN <b>{name}</b>")
        lay.addWidget(head)
        if why:
            w = QLabel(why)
            w.setWordWrap(True)
            w.setStyleSheet(f"color: {YELLOW};")
            lay.addWidget(w)
        lay.addLayout(f)
        lay.addWidget(self.save)
        lay.addWidget(bb)
        (self.pw if user else self.user).setFocus()


class VpnPanel(QWidget):
    """Onglet « VPN » du panneau de gauche."""

    def __init__(self, win):
        super().__init__()
        self.win = win
        self.vpns = []
        self.creds = {}
        self.busy = False
        self.has_nm = shutil.which("nmcli") is not None
        lay = QVBoxLayout(self)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.setSpacing(0)

        def tool(name, tip, fn, col=FG):
            b = QToolButton()
            b.setIcon(icon(name, col))
            b.setToolTip(tip)
            b.clicked.connect(fn)
            return b
        add = QPushButton("  Ajouter un VPN (.ovpn)")
        add.setIcon(icon("new", FG))
        add.clicked.connect(self.add)
        top = QHBoxLayout()
        top.setContentsMargins(6, 6, 6, 4)
        top.setSpacing(2)
        top.addWidget(add, 1)
        top.addWidget(tool("reload", "Actualiser", self.refresh))
        self.tree = QTreeWidget()
        self.tree.setHeaderHidden(True)
        self.tree.setRootIsDecorated(False)
        self.tree.setColumnCount(2)
        self.tree.setColumnWidth(0, 125)
        self.tree.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.tree.itemDoubleClicked.connect(lambda *_: self.toggle())
        self.tree.currentItemChanged.connect(lambda *_: self.update_buttons())
        self.b_conn = QPushButton("Connecter")
        self.b_conn.setObjectName("primary")
        self.b_conn.clicked.connect(self.toggle)
        self.b_del = tool("trash", "Supprimer ce VPN", self.delete, RED)
        self.b_key = tool("key", "Oublier les identifiants enregistrés", self.forget, YELLOW)
        row = QHBoxLayout()
        row.setContentsMargins(6, 4, 6, 4)
        row.setSpacing(2)
        row.addWidget(self.b_conn, 1)
        row.addWidget(self.b_key)
        row.addWidget(self.b_del)
        self.status = QLabel()
        self.status.setWordWrap(True)
        self.split = QCheckBox("Garder internet : VPN seulement pour le lab")
        self.split.setToolTip("Coché : seul le réseau du lab passe par le VPN, le reste d'internet passe par ta "
                              "connexion normale.\nDécoché : tout ton trafic passe par le VPN.")
        self.split.clicked.connect(self.opts_changed)
        self.routes = QLineEdit()
        self.routes.setPlaceholderText("réseaux du lab via le VPN, ex : 10.22.0.0/16")
        self.routes.setToolTip("Facultatif. Réseaux ou adresses à joindre par le VPN, séparés par des virgules.\n"
                               "Les réseaux annoncés par le serveur VPN et les machines de tes labs SSH liés "
                               "sont ajoutés tout seuls.")
        self.routes.editingFinished.connect(self.opts_changed)
        optbox = QVBoxLayout()
        optbox.setContentsMargins(8, 2, 8, 2)
        optbox.setSpacing(4)
        optbox.addWidget(self.split)
        optbox.addWidget(self.routes)
        lay.addLayout(top)
        lay.addWidget(self.tree, 1)
        lay.addLayout(optbox)
        lay.addLayout(row)
        lay.addWidget(self.status)
        self.timer = QTimer(self)
        self.timer.setInterval(5000)
        self.timer.timeout.connect(lambda: self.refresh() if self.isVisible() and not self.busy else None)
        if self.has_nm:
            self.set_status("", COMMENT)
            self.timer.start()
            self.refresh()
        else:
            self.set_status("NetworkManager (nmcli) est introuvable : le VPN ne peut pas être géré d'ici.", RED)
            add.setEnabled(False)
        self.update_buttons()

    # ----------------------------------------------------------- nmcli
    def nmcli(self, args, done, timeout=60000):
        """Lance nmcli sans bloquer l'interface ; done(code, sortie)."""
        p = QProcess(self)
        p.setProcessChannelMode(QProcess.ProcessChannelMode.MergedChannels)
        timer = QTimer(p)
        timer.setSingleShot(True)
        timer.timeout.connect(p.kill)
        timer.start(timeout)

        def finished(code, _status):
            out = bytes(p.readAllStandardOutput()).decode("utf-8", "replace")
            p.deleteLater()
            done(code, out)
        p.finished.connect(finished)
        p.errorOccurred.connect(lambda err: (p.deleteLater(), done(-1, "nmcli n'a pas pu être lancé"))
                                if err == QProcess.ProcessError.FailedToStart else None)
        env = p.processEnvironment()
        from PyQt6.QtCore import QProcessEnvironment
        env = QProcessEnvironment.systemEnvironment()
        env.insert("LC_ALL", "C")  # messages en anglais, pour les reconnaître
        p.setProcessEnvironment(env)
        p.start("nmcli", args)

    def set_status(self, text, col=FG):
        self.status.setText(text)
        self.status.setStyleSheet(f"padding: 5px 8px; color: {col};")

    def set_busy(self, on, text=""):
        self.busy = on
        if on:
            self.set_status(text, YELLOW)
        self.update_buttons()

    # ------------------------------------------------------------ liste
    def refresh(self, then=None):
        if not self.has_nm:
            return

        def done(code, out):
            if code == 0:
                self.vpns = parse_list(out)
                self.show_list()
            if then:
                then()
        self.nmcli(["-t", "-f", "NAME,UUID,TYPE,ACTIVE,STATE", "connection", "show"], done, 15000)

    def show_list(self):
        cur = self.current()
        keep = cur["uuid"] if cur else None
        self.tree.clear()
        for v in self.vpns:
            state = "connecté" if v["active"] else ("…" if v["busy"] else "")
            it = QTreeWidgetItem([v["name"], state])
            it.setIcon(0, icon("shield", GREEN if v["active"] else COMMENT))
            it.setForeground(1, QColor(GREEN if v["active"] else COMMENT))
            it.setData(0, Qt.ItemDataRole.UserRole, v["uuid"])
            self.tree.addTopLevelItem(it)
            if v["uuid"] == keep:
                self.tree.setCurrentItem(it)
        if self.tree.currentItem() is None and self.tree.topLevelItemCount():
            self.tree.setCurrentItem(self.tree.topLevelItem(0))
        if not self.busy:
            act = [v["name"] for v in self.vpns if v["active"]]
            if act:
                self.set_status("● VPN actif : " + ", ".join(act), GREEN)
            elif not self.vpns:
                self.set_status("Aucun VPN. Clique sur « Ajouter » et choisis le fichier .ovpn "
                                "fourni par ta fac.", COMMENT)
            elif self.status.text().startswith("●"):
                self.set_status("", COMMENT)
        self.update_buttons()
        if hasattr(self.win, "vpn_changed"):
            self.win.vpn_changed()

    def current(self):
        it = self.tree.currentItem()
        uuid = it.data(0, Qt.ItemDataRole.UserRole) if it else None
        return next((v for v in self.vpns if v["uuid"] == uuid), None)

    def active_name(self):
        return next((v["name"] for v in self.vpns if v["active"]), "")

    def by_name(self, name):
        return next((v for v in self.vpns if v["name"] == name), None)

    def names(self):
        return [v["name"] for v in self.vpns]

    def opts_changed(self):
        """La case ou les réseaux ont changé : on enregistre, et on reconnecte si le VPN est actif."""
        v = self.current()
        if not v:
            return
        o = vpn_opts(v["uuid"])
        new = dict(o, split=self.split.isChecked(), routes=", ".join(clean_routes(self.routes.text())))
        if new == o:
            return
        save_opts(v["uuid"], new)
        uuid = v["uuid"]

        def apply():  # état relu à l'instant : on ne reconnecte que si le VPN est vraiment actif
            cur = next((x for x in self.vpns if x["uuid"] == uuid), None)
            if cur and cur["active"] and not self.busy:
                self.reconnect(cur)
        self.refresh(apply)

    def reconnect(self, v, ok=None, err=None):
        self.set_busy(True, "Application des réglages : reconnexion du VPN…")

        def downed(code, out):
            self.busy = False
            v["active"] = False
            self.up(v, ok, err)
        self.nmcli(["connection", "down", "uuid", v["uuid"]], downed)

    def update_buttons(self):
        v = self.current()
        if v and not (self.routes.hasFocus()):
            o = vpn_opts(v["uuid"])
            self.split.setChecked(o["split"])
            if self.routes.text() != o["routes"]:
                self.routes.setText(o["routes"])
        self.split.setEnabled(bool(v))
        self.routes.setEnabled(bool(v) and self.split.isChecked())
        self.b_conn.setEnabled(bool(v) and not self.busy)
        self.b_del.setEnabled(bool(v) and not self.busy)
        self.b_key.setEnabled(bool(v))
        self.b_conn.setText("Déconnecter" if v and v["active"] else "Connecter")

    # ----------------------------------------------------------- ajout
    def add(self):
        path, _ = QFileDialog.getOpenFileName(self, "Fichier de configuration VPN", os.path.expanduser("~"),
                                              "OpenVPN (*.ovpn *.conf);;Tous les fichiers (*)")
        if not path:
            return
        try:
            with open(path, encoding="utf-8", errors="replace") as fh:
                conf = fh.read()
        except OSError as e:
            QMessageBox.warning(self, "VPN", f"Fichier illisible :\n{e}")
            return
        needs_login = re.search(r"^\s*auth-user-pass\b", conf, re.M) is not None
        self.set_busy(True, "Import du VPN…")

        def imported(code, out):
            m = re.search(r"\(([0-9a-f-]{36})\)", out)
            if code != 0 or not m:
                self.set_busy(False)
                self.set_status("Import impossible.", RED)
                QMessageBox.warning(self, "VPN", "Import impossible :\n\n" + nm_error(out))
                return
            uuid = m.group(1)
            # le mot de passe n'est pas stocké par NetworkManager : il vient du trousseau
            mods = ["+vpn.data", "password-flags=2"] if needs_login else []

            def finish(*_):
                self.set_busy(False)
                self.set_status("VPN ajouté. Sélectionne-le puis clique sur Connecter.", GREEN)
                self.refresh(lambda: self.select(uuid))
            if mods:
                self.nmcli(["connection", "modify", "uuid", uuid] + mods, finish)
            else:
                finish()
        self.nmcli(["connection", "import", "type", "openvpn", "file", path], imported)

    def select(self, uuid):
        for i in range(self.tree.topLevelItemCount()):
            it = self.tree.topLevelItem(i)
            if it.data(0, Qt.ItemDataRole.UserRole) == uuid:
                self.tree.setCurrentItem(it)

    def delete(self):
        v = self.current()
        if not v or QMessageBox.question(self, "Supprimer", f"Supprimer le VPN « {v['name']} » ?") \
                != QMessageBox.StandardButton.Yes:
            return
        pw_del("vpn-" + v["uuid"])
        self.set_busy(True, "Suppression…")
        self.nmcli(["connection", "delete", "uuid", v["uuid"]],
                   lambda c, o: (self.set_busy(False), self.set_status("", COMMENT), self.refresh()))

    def forget(self):
        v = self.current()
        if v:
            pw_del("vpn-" + v["uuid"])
            self.win.statusBar().showMessage("Identifiants du VPN oubliés.", 3000)

    # ------------------------------------------------- connexion / coupure
    def toggle(self):
        v = self.current()
        if not v or self.busy:
            return
        if v["active"]:
            self.down(v)
        else:
            self.up(v)

    def down(self, v):
        self.set_busy(True, f"Déconnexion de {v['name']}…")
        self.nmcli(["connection", "down", "uuid", v["uuid"]],
                   lambda c, o: (self.set_busy(False), self.set_status("VPN déconnecté.", COMMENT), self.refresh()))

    def up(self, v, ok=None, err=None, ask=False, why=""):
        """Active le VPN ; ok() / err(message) sont appelés à la fin."""
        if self.busy:
            if err:
                err("une opération VPN est déjà en cours")
            return

        def fail(msg):
            self.set_busy(False)
            self.set_status("Échec : " + msg.splitlines()[0], RED)
            self.refresh()
            if err:
                err(msg)
            else:
                QMessageBox.warning(self, "VPN", msg)

        def start(user, password, save):
            self.set_busy(True, f"Connexion au VPN {v['name']}…")
            args = ["--wait", "45", "connection", "up", "uuid", v["uuid"]]
            tmp = None
            if password:
                fd, tmp = tempfile.mkstemp(prefix="sc-vpn-")
                with os.fdopen(fd, "w") as fh:  # fichier privé (0600), supprimé juste après
                    fh.write("vpn.secrets.password:%s\n" % password)
                args += ["passwd-file", tmp]

            def done(code, out):
                if tmp:
                    try:
                        os.remove(tmp)
                    except OSError:
                        pass
                if code == 0:
                    if password:  # gardés pour la session : une reconnexion ne les redemande pas
                        self.creds[v["uuid"]] = user + "\n" + password
                    if save and password:
                        pw_set("vpn-" + v["uuid"], user + "\n" + password)
                    for x in self.vpns:  # état à jour tout de suite, sans attendre la relecture
                        if x["uuid"] == v["uuid"]:
                            x["active"] = True
                    self.set_busy(False)
                    self.set_status("● VPN actif : " + v["name"], GREEN)
                    self.refresh()
                    if ok:
                        ok()
                    return
                msg = nm_error(out)
                if "refusé" in msg and (password or not ask):
                    pw_del("vpn-" + v["uuid"])
                    self.creds.pop(v["uuid"], None)
                    self.set_busy(False)
                    self.up(v, ok, err, ask=True, why="Identifiants refusés — réessaie." if password else "")
                    return
                fail(msg)

            def connect(*_):
                self.nmcli(args, done, 70000)

            def go(*_):  # d'abord les réglages de routage (garder internet ou non), puis la connexion
                self.nmcli(["connection", "modify", "uuid", v["uuid"]] + route_args(v["uuid"]), connect)
            if user:
                self.nmcli(["connection", "modify", "uuid", v["uuid"], "+vpn.data", "username=" + user], go)
            else:
                go()

        saved = None if ask else (pw_get("vpn-" + v["uuid"]) or self.creds.get(v["uuid"]))
        if saved and "\n" in saved:
            user, password = saved.split("\n", 1)
            start(user, password, False)
        elif ask:
            d = CredDialog(self, v["name"], why=why)
            if not d.exec():
                self.set_status("Connexion annulée.", COMMENT)
                if err:
                    err("connexion au VPN annulée")
                return
            start(d.user.text().strip(), d.pw.text(), d.save.isChecked())
        else:
            start("", "", False)  # VPN à certificat seul, ou secrets déjà connus du système

    def ensure_up(self, name, ok, err, host=None):
        """Pour les labs SSH : active le VPN `name` s'il ne l'est pas déjà, et s'assure que la
        machine `host` passe bien par lui quand le VPN ne sert qu'au lab."""
        if not self.has_nm:
            return err("NetworkManager est introuvable")

        def check():
            v = self.by_name(name)
            if v is None:
                return err(f"le VPN « {name} » n'existe plus (onglet VPN → Ajouter)")
            added = False
            if host:
                try:
                    ip = str(ipaddress.ip_address(host))
                except ValueError:
                    ip = None
                o = vpn_opts(v["uuid"])
                if ip and ":" not in ip and o["split"] and ip + "/32" not in clean_routes(
                        o["routes"] + " " + " ".join(o["hosts"])):
                    save_opts(v["uuid"], dict(o, hosts=(o["hosts"] + [ip])[-40:]))
                    added = True
            if v["active"] and added:
                self.reconnect(v, ok, err)   # la nouvelle route ne s'applique qu'à la connexion
            elif v["active"]:
                ok()
            else:
                self.up(v, ok, err)
        self.refresh(check)


# ============================================================== Windows
class WinVpnPanel(QWidget):
    """Onglet VPN sous Windows : s'appuie sur OpenVPN GUI (client OpenVPN officiel), qui gère
    lui-même l'identifiant et le mot de passe. SCode range les fichiers .ovpn dans le
    dossier de configurations d'OpenVPN GUI et lui envoie les ordres de connexion."""
    DOWNLOAD = "https://openvpn.net/community-downloads/"

    def __init__(self, win):
        super().__init__()
        self.win = win
        self.busy = False
        self.active = False
        self.vpns = []
        self.gui = self.find_gui()
        self.conf_dir = os.path.join(os.path.expanduser("~"), "OpenVPN", "config")
        lay = QVBoxLayout(self)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.setSpacing(0)
        add = QPushButton("  Ajouter un VPN (.ovpn)")
        add.setIcon(icon("new", FG))
        add.clicked.connect(self.add)
        top = QHBoxLayout()
        top.setContentsMargins(6, 6, 6, 4)
        top.addWidget(add, 1)
        self.tree = QTreeWidget()
        self.tree.setHeaderHidden(True)
        self.tree.setRootIsDecorated(False)
        self.tree.setColumnCount(2)
        self.tree.setColumnWidth(0, 125)
        self.tree.itemDoubleClicked.connect(lambda *_: self.toggle())
        self.split = QCheckBox("Garder internet : VPN seulement pour le lab")
        self.split.setToolTip("Coché : le VPN ne remplace pas ta connexion internet ; seuls les réseaux "
                              "annoncés par le serveur et ceux indiqués ci-dessous passent par lui.")
        self.split.clicked.connect(self.opts_changed)
        self.routes = QLineEdit()
        self.routes.setPlaceholderText("réseaux du lab via le VPN, ex : 10.11.0.0/28")
        self.routes.editingFinished.connect(self.opts_changed)
        self.b_conn = QPushButton("Connecter")
        self.b_conn.setObjectName("primary")
        self.b_conn.clicked.connect(self.toggle)
        self.b_del = QToolButton()
        self.b_del.setIcon(icon("trash", RED))
        self.b_del.setToolTip("Supprimer ce VPN")
        self.b_del.clicked.connect(self.delete)
        row = QHBoxLayout()
        row.setContentsMargins(6, 4, 6, 4)
        row.addWidget(self.b_conn, 1)
        row.addWidget(self.b_del)
        opt = QVBoxLayout()
        opt.setContentsMargins(8, 2, 8, 2)
        opt.addWidget(self.split)
        opt.addWidget(self.routes)
        self.status = QLabel()
        self.status.setWordWrap(True)
        lay.addLayout(top)
        lay.addWidget(self.tree, 1)
        lay.addLayout(opt)
        lay.addLayout(row)
        lay.addWidget(self.status)
        if not self.gui:
            self.set_status("OpenVPN GUI n'est pas installé : installe « OpenVPN Community » "
                            "(bouton ci-dessous), puis relance SCode.", YELLOW)
            dl = QPushButton("Télécharger OpenVPN")
            dl.clicked.connect(lambda: QDesktopServices.openUrl(QUrl(self.DOWNLOAD)))
            lay.addWidget(dl)
            add.setEnabled(False)
        self.tree.currentItemChanged.connect(lambda *_: self.update_buttons())
        self.timer = QTimer(self)
        self.timer.setInterval(5000)
        self.timer.timeout.connect(lambda: self.check_active() if self.isVisible() else None)
        self.timer.start()
        self.refresh()
        self.check_active()

    @staticmethod
    def find_gui():
        for p in (shutil.which("openvpn-gui"),
                  os.path.join(os.environ.get("ProgramFiles", r"C:\Program Files"), "OpenVPN", "bin", "openvpn-gui.exe"),
                  os.path.join(os.environ.get("ProgramFiles(x86)", r"C:\Program Files (x86)"), "OpenVPN", "bin",
                               "openvpn-gui.exe")):
            if p and os.path.isfile(p):
                return p
        return None

    def set_status(self, text, col=FG):
        self.status.setText(text)
        self.status.setStyleSheet(f"padding: 5px 8px; color: {col};")

    # ------------------------------------------------------------ fichiers
    def refresh(self, then=None):
        found = []
        if os.path.isdir(self.conf_dir):
            for d, _dirs, files in os.walk(self.conf_dir):
                for f in files:
                    if f.lower().endswith(".ovpn"):
                        name = f[:-5]
                        found.append({"name": name, "uuid": name, "path": os.path.join(d, f),
                                      "active": self.active and name == self.last()})
        self.vpns = sorted(found, key=lambda v: v["name"].lower())
        cur = self.current()
        self.tree.clear()
        for v in self.vpns:
            it = QTreeWidgetItem([v["name"], "connecté" if v["active"] else ""])
            it.setIcon(0, icon("shield", GREEN if v["active"] else COMMENT))
            it.setForeground(1, QColor(GREEN))
            it.setData(0, Qt.ItemDataRole.UserRole, v["name"])
            self.tree.addTopLevelItem(it)
            if cur and cur["name"] == v["name"]:
                self.tree.setCurrentItem(it)
        if self.tree.currentItem() is None and self.tree.topLevelItemCount():
            self.tree.setCurrentItem(self.tree.topLevelItem(0))
        self.update_buttons()
        if then:
            then()

    def current(self):
        it = self.tree.currentItem()
        name = it.data(0, Qt.ItemDataRole.UserRole) if it else None
        return next((v for v in self.vpns if v["name"] == name), None)

    def by_name(self, name):
        return next((v for v in self.vpns if v["name"] == name), None)

    def names(self):
        return [v["name"] for v in self.vpns]

    def last(self):
        return QSettings("StudentCode", "StudentCode").value("win_vpn_last", "")

    def active_name(self):
        return self.last() if self.active else ""

    def update_buttons(self):
        v = self.current()
        self.b_conn.setEnabled(bool(v and self.gui) and not self.busy)
        self.b_del.setEnabled(bool(v))
        self.b_conn.setText("Déconnecter" if v and v["active"] else "Connecter")
        if v:
            o = vpn_opts(v["uuid"])
            self.split.setChecked(o["split"])
            if not self.routes.hasFocus():
                self.routes.setText(o["routes"])
        self.split.setEnabled(bool(v))
        self.routes.setEnabled(bool(v) and self.split.isChecked())

    def write_config(self, name):
        """Régénère <nom>.ovpn depuis le fichier d'origine, avec les réglages de SCode."""
        folder = os.path.join(self.conf_dir, name)
        source = os.path.join(folder, name + ".source")
        target = os.path.join(folder, name + ".ovpn")
        if not os.path.isfile(source):
            return
        with open(source, encoding="utf-8", errors="replace") as fh:
            text = fh.read().rstrip("\n") + "\n"
        o = vpn_opts(name)
        if o["split"]:
            text += "\n# --- ajouté par StudentCode : internet ne passe pas par le VPN ---\n"
            text += 'pull-filter ignore "redirect-gateway"\n'
            for net in clean_routes(o["routes"] + " " + " ".join(o["hosts"])):
                n = ipaddress.ip_network(net)
                text += f"route {n.network_address} {n.netmask}\n"
        with open(target, "w", encoding="utf-8") as fh:
            fh.write(text)

    def add(self):
        path, _ = QFileDialog.getOpenFileName(self, "Fichier de configuration VPN", os.path.expanduser("~"),
                                              "OpenVPN (*.ovpn *.conf);;Tous les fichiers (*)")
        if not path:
            return
        name = re.sub(r"[^\w.-]+", "_", os.path.splitext(os.path.basename(path))[0]) or "vpn"
        folder = os.path.join(self.conf_dir, name)
        try:
            os.makedirs(folder, exist_ok=True)
            shutil.copyfile(path, os.path.join(folder, name + ".source"))
            # fichiers voisins éventuels (certificats séparés)
            src_dir = os.path.dirname(path)
            for f in os.listdir(src_dir):
                if f.lower().endswith((".crt", ".key", ".pem", ".p12", ".txt")):
                    shutil.copyfile(os.path.join(src_dir, f), os.path.join(folder, f))
            self.write_config(name)
        except OSError as e:
            QMessageBox.warning(self, "VPN", f"Import impossible :\n{e}")
            return
        self.set_status("VPN ajouté. Sélectionne-le puis clique sur Connecter.", GREEN)
        self.refresh()

    def delete(self):
        v = self.current()
        if v and QMessageBox.question(self, "Supprimer", f"Supprimer le VPN « {v['name']} » ?") \
                == QMessageBox.StandardButton.Yes:
            if v["active"]:
                self.gui_command("disconnect", v["name"])
            shutil.rmtree(os.path.dirname(v["path"]), ignore_errors=True)
            self.refresh()

    def opts_changed(self):
        v = self.current()
        if not v:
            return
        o = vpn_opts(v["uuid"])
        new = dict(o, split=self.split.isChecked(), routes=", ".join(clean_routes(self.routes.text())))
        if new != o:
            save_opts(v["uuid"], new)
            self.write_config(v["name"])
            if v["active"]:
                self.set_status("Réglage enregistré : reconnecte le VPN pour l'appliquer.", YELLOW)

    # --------------------------------------------------------- connexion
    def gui_command(self, action, name):
        """Envoie un ordre à OpenVPN GUI (le lance s'il ne tourne pas encore)."""
        if action == "connect":
            QSettings("StudentCode", "StudentCode").setValue("win_vpn_last", name)
            running = QProcess.execute("cmd", ["/c", "tasklist /FI \"IMAGENAME eq openvpn-gui.exe\" | find /I \"openvpn-gui\""]) == 0
            if not running:
                return QProcess.startDetached(self.gui, ["--connect", name + ".ovpn"])
        return QProcess.startDetached(self.gui, ["--command", action, name])

    def toggle(self):
        v = self.current()
        if not v:
            return
        if v["active"]:
            self.gui_command("disconnect", v["name"])
            self.set_status("Déconnexion demandée à OpenVPN.", COMMENT)
        else:
            self.write_config(v["name"])
            self.gui_command("connect", v["name"])
            self.set_status("Connexion en cours : OpenVPN peut demander ton identifiant dans sa fenêtre.", YELLOW)
        QTimer.singleShot(4000, self.check_active)

    def check_active(self, then=None):
        """Un adaptateur OpenVPN est-il connecté ? (PowerShell, sans bloquer l'interface)"""
        p = QProcess(self)
        script = ("(Get-NetAdapter | Where-Object { $_.InterfaceDescription -match 'TAP-Windows|OpenVPN|Wintun' "
                  "-and $_.Status -eq 'Up' } | Measure-Object).Count")

        def done(*_):
            out = bytes(p.readAllStandardOutput()).decode(errors="replace").strip()
            p.deleteLater()
            was = self.active
            self.active = out.isdigit() and int(out) > 0
            if self.active != was:
                self.set_status(("● VPN actif : " + self.last()) if self.active else "", GREEN)
            self.refresh()
            if then:
                then(self.active)
        p.finished.connect(done)
        p.start("powershell", ["-NoProfile", "-Command", script])

    def ensure_up(self, name, ok, err, host=None):
        """Pour les labs SSH : active le VPN `name` et attend qu'il soit connecté."""
        if not self.gui:
            return err("OpenVPN GUI n'est pas installé")
        if self.by_name(name) is None:
            return err(f"le VPN « {name} » n'existe plus (onglet VPN → Ajouter)")
        if host:
            try:
                ip = str(ipaddress.ip_address(host))
                o = vpn_opts(name)
                if ip + "/32" not in clean_routes(o["routes"] + " " + " ".join(o["hosts"])):
                    save_opts(name, dict(o, hosts=(o["hosts"] + [ip])[-40:]))
                    self.write_config(name)
            except ValueError:
                pass
        tries = [0]

        def check(active):
            if active:
                return ok()
            tries[0] += 1
            if tries[0] > 20:
                return err("le VPN ne s'est pas connecté (vérifie la fenêtre d'OpenVPN)")
            QTimer.singleShot(2000, lambda: self.check_active(check))

        def first(active):
            if active and self.last() == name:
                return ok()
            self.gui_command("connect", name)
            QTimer.singleShot(3000, lambda: self.check_active(check))
        self.check_active(first)


if os.name == "nt":
    VpnPanel = WinVpnPanel   # noqa: F811 — sous Windows, l'onglet VPN passe par OpenVPN GUI
