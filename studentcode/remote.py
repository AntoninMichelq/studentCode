"""Connexion SSH de SCode : labs enregistrés, explorateur distant,
édition et exécution sur la machine, console.

Une seule connexion par lab (SFTP, exécution et console passent par le même
transport) : pas de sessions en double côté machine.
"""
import base64
import codecs
import hashlib
import json
import os
import posixpath
import queue
import re
import shlex
import shutil
import socket
import stat
import subprocess
import threading
import uuid

from PyQt6.QtCore import QObject, QProcess, QSettings, Qt, QTimer, pyqtSignal
from PyQt6.QtGui import QColor
from PyQt6.QtWidgets import (QApplication, QCheckBox, QComboBox, QDialog, QDialogButtonBox,
                             QFileDialog, QFormLayout, QHBoxLayout, QInputDialog, QLabel,
                             QLineEdit, QMenu, QMessageBox, QPushButton, QSpinBox, QTabBar, QToolButton,
                             QTreeWidget, QTreeWidgetItem, QVBoxLayout, QWidget)

from icons import file_icon, icon

from lazy import LazyModule, available

HAS_SSH = available("paramiko")
paramiko = LazyModule("paramiko")   # ~30 Mo, chargé seulement à la première connexion

from theme import BG_HL, BLUE, COMMENT, CYAN, FG, GREEN, RED, YELLOW
SERVICE = "studentcode-ssh"
MAX_EDIT = 5 * 1024 * 1024
DATA_DIR = os.path.join(os.path.expanduser("~"), ".config", "StudentCode")
KNOWN_HOSTS = os.path.join(DATA_DIR, "known_hosts")
CACHE_DIR = os.path.join(os.path.expanduser("~"), ".cache", "studentcode", "ssh")
ANSI = re.compile(r"\x1b\][^\x07\x1b]*(\x07|\x1b\\)|\x1b\[[0-9;?]*[ -/]*[@-~]|\x1b[=>()][0-9A-B]?|\r")

# Ferme les anciennes sessions SSH du même utilisateur et ses programmes restés
# attachés à un terminal, sans jamais toucher à la connexion en cours.
# Nettoyage à la connexion : ferme ce qui traîne d'une séance précédente et provoque des
# conflits, sans jamais toucher à la connexion en cours ni aux services du système.
#   - les anciennes sessions SSH de l'utilisateur et ce qui y tournait (terminaux pts) ;
#   - ses programmes restés en écoute sur un port (serveur de TP → « Address already in use ») ;
#   - ses programmes de développement en arrière-plan : java (dont les démons Gradle), python,
#     node, gdb, valgrind… (pour root : uniquement java, gradle, node et les terminaux).
# Chaque processus reçoit d'abord SIGTERM, puis SIGKILL s'il est encore là une seconde après.
CLEANUP = r'''
me=$(id -un); uid=$(id -u); anc=" "; p=$$
while [ -n "$p" ] && [ "$p" -gt 1 ] 2>/dev/null; do
  anc="$anc$p "; p=$(ps -o ppid= -p "$p" 2>/dev/null | tr -d ' ')
done
keep='^(systemd|\(sd-pam\)|sd-pam|dbus-daemon|dbus-broker|pipewire|pipewire-pulse|wireplumber|gpg-agent|ssh-agent|gnome-keyring-daemon|at-spi.*|xdg-.*|snapd.*|polkit.*|bash|sh|dash|zsh|sudo|su|login|tmux: server|screen)$'
found=" "
add() {
  case "$anc$found" in *" $1 "*) return;; esac
  [ "$(ps -o ppid= -p "$1" 2>/dev/null | tr -d ' ')" = "$$" ] && return
  comm=$(ps -o comm= -p "$1" 2>/dev/null) || return
  case "$2" in keepcheck) echo "$comm" | grep -Eq "$keep" && return;; esac
  found="$found$1 "
}
# 1. anciennes sessions SSH, et ce qui tourne dans un terminal (pts)
for x in $(pgrep -u "$me" -f "^sshd(-session)?: $me" 2>/dev/null); do add "$x"; done
for x in $(ps -u "$me" -o pid=,tty= 2>/dev/null | awk '$2 ~ /^pts/ {print $1}'); do add "$x"; done
# 2. programmes de l'utilisateur en écoute sur un port
if command -v ss >/dev/null 2>&1; then
  for x in $(ss -ltnpuH 2>/dev/null | grep -o 'pid=[0-9]*' | cut -d= -f2 | sort -u); do
    [ "$(ps -o uid= -p "$x" 2>/dev/null | tr -d ' ')" = "$uid" ] || continue
    case "$(ps -o comm= -p "$x" 2>/dev/null)" in sshd*|systemd*) continue;; esac
    add "$x" keepcheck
  done
fi
# 3. programmes de développement restés en arrière-plan
if [ "$uid" = 0 ]; then dev='^(java|javac|gradle|kotlin|node|npm|nodemon)$'
else dev='^(java|javac|gradle|gradlew|kotlin|kotlinc|mvn|python|python3|python3\.[0-9]+|node|npm|npx|nodemon|gdb|valgrind|make|nc|ncat|socat|telnet)$'; fi
for x in $(ps -u "$me" -o pid=,comm= 2>/dev/null | awk -v re="$dev" '$2 ~ re {print $1}'); do add "$x"; done
# arrêt propre, puis forcé pour ceux qui résistent
for x in $found; do
  name=$(ps -o args= -p "$x" 2>/dev/null | cut -c1-70)
  kill "$x" 2>/dev/null && echo "$x $name"
done
[ "$found" = " " ] || sleep 1
for x in $found; do kill -0 "$x" 2>/dev/null && kill -9 "$x" 2>/dev/null; done
exit 0
'''


# ------------------------------------------------------------- trousseau
def _keyring():
    try:
        import keyring
        from keyring.backends import fail
        if isinstance(keyring.get_keyring(), fail.Keyring):
            return None
        return keyring
    except Exception:
        return None


def pw_available():
    return _keyring() is not None


def pw_get(key):
    try:
        return _keyring().get_password(SERVICE, key)
    except Exception:
        return None


def pw_set(key, value):
    try:
        _keyring().set_password(SERVICE, key, value)
        return True
    except Exception:
        return False


def pw_del(key):
    try:
        _keyring().delete_password(SERVICE, key)
    except Exception:
        pass


# ------------------------------------------------------------------ SSH
class UnknownHost(Exception):
    def __init__(self, name, key):
        super().__init__(name)
        self.name, self.key = name, key


_policy = None


def AskPolicy():
    """Politique « machine inconnue » : on remonte la question à l'interface."""
    global _policy
    if _policy is None:
        class _AskPolicy(paramiko.MissingHostKeyPolicy):
            def missing_host_key(self, client, hostname, key):
                raise UnknownHost(hostname, key)
        _policy = _AskPolicy
    return _policy()


def fingerprint(key):
    return "SHA256:" + base64.b64encode(hashlib.sha256(key.asbytes()).digest()).decode().rstrip("=")


def host_id(host, port):
    return host if int(port) == 22 else "[%s]:%d" % (host, int(port))


def trust_host(name, key):
    os.makedirs(DATA_DIR, exist_ok=True)
    hk = paramiko.HostKeys()
    if os.path.isfile(KNOWN_HOSTS):
        hk.load(KNOWN_HOSTS)
    if name in hk:
        del hk[name]
    hk.add(name, key.get_name(), key)
    hk.save(KNOWN_HOSTS)
    os.chmod(KNOWN_HOSTS, 0o600)


def friendly(e):
    """Message d'erreur lisible."""
    ssh_loaded = HAS_SSH and paramiko._mod is not None
    if ssh_loaded and isinstance(e, paramiko.AuthenticationException):
        return "Identifiant ou mot de passe refusé par la machine."
    if ssh_loaded and isinstance(e, paramiko.ssh_exception.NoValidConnectionsError):
        inner = list(getattr(e, "errors", {}).values())
        if inner:
            return friendly(inner[0])
        return "La machine est injoignable."
    if isinstance(e, PermissionError) and getattr(e, "errno", None) == 13 and "connect" in str(e).lower():
        return "Cette adresse n'est pas celle d'une machine (adresse de diffusion ?). Vérifie l'IP avec « ip addr »."
    if isinstance(e, OSError) and getattr(e, "errno", None) in (101, 113):
        return "Réseau injoignable : es-tu sur le réseau de la fac ou sur le VPN ?"
    if isinstance(e, (socket.timeout, TimeoutError)):
        return "La machine ne répond pas (lab démarré ? VPN ou réseau de la fac actif ?)."
    if isinstance(e, socket.gaierror):
        return "Adresse introuvable : vérifie le nom ou l'IP de la machine."
    if isinstance(e, ConnectionRefusedError):
        return "Connexion refusée : la machine est allumée mais SSH n'écoute pas sur ce port."
    if isinstance(e, PermissionError):
        return "Permission refusée sur la machine pour cette opération."
    if isinstance(e, FileNotFoundError):
        return "Fichier ou dossier introuvable sur la machine."
    if isinstance(e, EOFError):
        return "La connexion a été coupée."
    text = str(e) or e.__class__.__name__
    return text if len(text) < 300 else text[:300] + "…"


def run_command(path, python="python3 -u", unbuffer=True, sep="/"):
    """Commande bash qui exécute `path` sur une machine Linux (voir runcmd)."""
    from runcmd import posix_command
    return posix_command(path, python, unbuffer)


ENABLE_SSH = "sudo apt install -y openssh-server && sudo systemctl enable --now ssh"
ENABLE_PASSWORD = ("sudo sed -i 's/^#\\?PasswordAuthentication.*/PasswordAuthentication yes/' "
                   "/etc/ssh/sshd_config /etc/ssh/sshd_config.d/*.conf && sudo systemctl restart ssh")


def diagnose(host, port):
    """Cherche pourquoi la connexion échoue : route (VPN ou non), port SSH, ping.
    Renvoie (résumé court, explication avec la marche à suivre)."""
    from sysinfo import WINDOWS
    dev = ""
    if not WINDOWS:   # interface utilisée pour joindre la machine (VPN ou non)
        try:
            out = subprocess.run(["ip", "-4", "route", "get", host], capture_output=True, text=True, timeout=4).stdout
            m = re.search(r"\bdev (\S+)", out)
            dev = m.group(1) if m else ""
        except Exception:
            pass
    via_vpn = dev.startswith(("tun", "tap", "wg", "ppp", "vpn"))
    try:
        socket.create_connection((host, int(port)), timeout=4).close()
        tcp = "open"
    except ConnectionRefusedError:
        tcp = "refused"
    except OSError:
        tcp = "timeout"
    if tcp == "open":
        return ("Le port SSH répond, mais la connexion a échoué.",
                "La machine est joignable et SSH écoute : réessaie, puis vérifie l'utilisateur.")
    if tcp == "refused":
        return ("La machine répond, mais SSH n'y est pas démarré.",
                f"Le port {port} est fermé sur {host}. Dans la console de la machine (RemoteLabz), lance :\n"
                f"    {ENABLE_SSH}\npuis reconnecte-toi.")
    try:
        cmd = ["ping", "-n", "1", "-w", "2000", host] if WINDOWS else ["ping", "-c", "1", "-W", "2", host]
        ping = subprocess.run(cmd, capture_output=True, timeout=5,
                              creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0)).returncode == 0
    except Exception:
        ping = False
    private = False
    try:
        import ipaddress
        private = ipaddress.ip_address(host).is_private
    except ValueError:
        pass
    if private and dev and not via_vpn and not ping:
        return ("Cette adresse ne passe pas par le VPN.",
                f"Ton PC envoie {host} par l'interface « {dev or '?'} », pas par le VPN. Vérifie dans l'onglet VPN "
                "que le VPN de CE lab est connecté (chaque lab RemoteLabz a son propre fichier OpenVPN), "
                "puis réessaie.")
    if ping:
        return ("La machine répond au ping, mais pas en SSH.",
                f"{host} est joignable mais le port {port} ne répond pas : SSH n'est sans doute pas installé "
                f"ou un pare-feu le bloque. Dans la console de la machine :\n    {ENABLE_SSH}")
    return ("La machine ne répond pas.",
            f"{host} ne répond ni au ping ni en SSH" + (" alors que le VPN est actif" if via_vpn else "") +
            ". Vérifie que la machine est démarrée, que l'adresse est la bonne (commande « ip addr » dans sa "
            "console) et que le VPN connecté est celui de ce lab.")


class SSH(QObject):
    """Connexion unique ; les opérations bloquantes passent par une file dans un thread."""
    _done = pyqtSignal(object, object, object)
    lost = pyqtSignal(str)
    run_out = pyqtSignal(str)
    run_end = pyqtSignal(int)
    shell_raw = pyqtSignal(bytes)
    shell_ready = pyqtSignal()

    def __init__(self):
        super().__init__()
        self.client = self.sftp = self.jump = None
        self.run_ch = self.shell_ch = None
        self.q = queue.Queue()
        self._done.connect(self._deliver)
        threading.Thread(target=self._loop, daemon=True).start()

    # ---- file d'attente
    def submit(self, fn, ok=None, err=None):
        self.q.put((fn, ok, err))

    def _loop(self):
        while True:
            fn, ok, err = self.q.get()
            try:
                self._emit(ok, fn(), None)
            except Exception as e:  # tout est remonté proprement à l'interface
                self._emit(err, None, e)
                if self.client is not None and not self.is_connected():
                    try:
                        self.lost.emit(friendly(e))
                    except RuntimeError:
                        pass

    def _emit(self, cb, result, exc):
        try:
            self._done.emit(cb, result, exc)
        except RuntimeError:  # application en cours de fermeture
            pass

    def _deliver(self, cb, result, exc):
        if cb is not None:
            cb(exc if exc is not None else result)

    # ---- connexion (appelée dans le thread)
    def is_connected(self):
        try:
            t = self.client.get_transport() if self.client else None
            return bool(t and t.is_active())
        except Exception:
            return False

    def _client(self):
        c = paramiko.SSHClient()
        if os.path.isfile(KNOWN_HOSTS):
            c.load_host_keys(KNOWN_HOSTS)
        c.set_missing_host_key_policy(AskPolicy())
        return c

    def connect_(self, prof, password):
        self.close()
        auth = dict(username=prof["user"], timeout=10, banner_timeout=15, auth_timeout=20)
        if prof.get("auth") == "key":
            auth.update(key_filename=os.path.expanduser(prof.get("key") or "") or None,
                        passphrase=password or None, look_for_keys=True, allow_agent=True)
        else:
            auth.update(password=password or "", look_for_keys=False, allow_agent=False)
        sock = None
        if prof.get("jump_host"):
            j = self._client()
            jauth = dict(auth, username=prof.get("jump_user") or prof["user"])
            j.connect(prof["jump_host"], port=int(prof.get("jump_port") or 22), **jauth)
            self.jump = j
            sock = j.get_transport().open_channel(
                "direct-tcpip", (prof["host"], int(prof["port"])), ("127.0.0.1", 0), timeout=10)
        c = self._client()
        try:
            c.connect(prof["host"], port=int(prof["port"]), sock=sock, **auth)
        except Exception:
            c.close()
            self._close_jump()
            raise
        c.get_transport().set_keepalive(25)
        self.client = c
        cleaned = []
        if cleanup_on(prof):
            try:   # un nettoyage qui échoue ou traîne ne doit jamais empêcher la connexion
                out, _ = self.exec_(CLEANUP, timeout=20)
                cleaned = [ln.strip() for ln in out.splitlines() if ln.strip()]
            except Exception as e:
                cleaned = ["(nettoyage impossible : %s)" % friendly(e)]
        self.sftp = c.open_sftp()
        self.sftp.get_channel().settimeout(25)
        return {"home": self.sftp.normalize("."), "cleaned": cleaned}

    def exec_(self, cmd, timeout=15):
        _, out, _ = self.client.exec_command(cmd, timeout=timeout)
        data = out.read().decode("utf-8", "replace")
        return data, out.channel.recv_exit_status()

    def _close_jump(self):
        if self.jump is not None:
            try:
                self.jump.close()
            except Exception:
                pass
            self.jump = None

    def close(self):
        for ch in (self.run_ch, self.shell_ch):
            try:
                if ch is not None:
                    ch.close()
            except Exception:
                pass
        self.run_ch = self.shell_ch = None
        for obj in (self.sftp, self.client):
            try:
                if obj is not None:
                    obj.close()
            except Exception:
                pass
        self.sftp = self.client = None
        self._close_jump()

    # ---- exécution distante et console (un thread lecteur chacun)
    def _pump(self, ch, signal):
        dec = codecs.getincrementaldecoder("utf-8")("replace")
        while True:
            data = ch.recv(4096)
            if not data:
                break
            text = ANSI.sub("", dec.decode(data))
            if text:
                signal.emit(text)

    def start_run(self, cmd):
        def work():
            rc = -1
            try:
                ch = self.client.get_transport().open_session(timeout=10)
                ch.get_pty(term="dumb", width=200, height=50)
                ch.exec_command(cmd)
                self.run_ch = ch
                self._pump(ch, self.run_out)
                rc = ch.recv_exit_status()
            except Exception as e:
                try:
                    self.run_out.emit("\n" + friendly(e) + "\n")
                except RuntimeError:
                    return
            self.run_ch = None
            try:
                self.run_end.emit(rc)
            except RuntimeError:
                pass
        threading.Thread(target=work, daemon=True).start()

    def run_send(self, text):
        try:
            if self.run_ch is not None:
                self.run_ch.sendall(text.encode("utf-8"))
        except Exception:
            pass

    def run_stop(self):
        ch = self.run_ch
        if ch is not None:
            try:
                ch.sendall(b"\x03")
                ch.close()
            except Exception:
                pass

    def start_shell(self, cols=200, rows=50, kind="xterm-256color"):
        def work():
            try:
                ch = self.client.invoke_shell(term=kind, width=cols, height=rows)
                self.shell_ch = ch
                self.shell_ready.emit()
                while True:
                    data = ch.recv(8192)
                    if not data:
                        break
                    self.shell_raw.emit(data)
            except Exception:
                pass
            self.shell_ch = None
        threading.Thread(target=work, daemon=True).start()

    def shell_send(self, data):
        try:
            if self.shell_ch is not None:
                self.shell_ch.sendall(data)
                return True
        except Exception:
            pass
        return False

    def shell_resize(self, cols, rows):
        try:
            if self.shell_ch is not None:
                self.shell_ch.resize_pty(width=cols, height=rows)
        except Exception:
            pass


class ExtraShell(QObject):
    """Un terminal SSH de plus sur une connexion déjà ouverte (autre canal, même machine)."""
    raw = pyqtSignal(bytes)
    ready = pyqtSignal()

    def __init__(self, client, cols=200, rows=50, kind="xterm-256color", start_dir=None):
        super().__init__()
        self.ch = None
        self.closed = False
        self.pending = b""          # ce qui est tapé avant que le shell soit prêt
        self.lock = threading.Lock()

        def work():
            try:
                ch = client.invoke_shell(term=kind, width=cols, height=rows)
                if start_dir:   # espace devant : la commande n'encombre pas l'historique
                    ch.sendall((" cd %s && clear\n" % shlex.quote(start_dir)).encode("utf-8"))
                with self.lock:
                    self.ch = ch
                    if self.pending:
                        ch.sendall(self.pending)
                        self.pending = b""
                self.ready.emit()
                while True:
                    data = ch.recv(8192)
                    if not data:
                        break
                    self.raw.emit(data)
            except Exception as e:
                if self.closed:
                    return
                try:
                    self.raw.emit(("\r\n" + friendly(e) + "\r\n").encode("utf-8"))
                except RuntimeError:
                    return
            self.ch = None
            if not self.closed:
                try:
                    self.raw.emit(b"\r\n[session SSH terminee]\r\n")
                except RuntimeError:
                    pass
        threading.Thread(target=work, daemon=True).start()

    def send(self, data):
        try:
            with self.lock:
                if self.ch is not None:
                    self.ch.sendall(data)
                elif not self.closed:
                    self.pending += data
        except Exception:
            pass

    def resize(self, cols, rows):
        try:
            if self.ch is not None:
                self.ch.resize_pty(width=cols, height=rows)
        except Exception:
            pass

    def close(self):
        self.closed = True
        try:
            if self.ch is not None:
                self.ch.close()
        except Exception:
            pass
        self.ch = None


# ---------------------------------------------------------------- labs
def load_labs():
    try:
        v = json.loads(QSettings("StudentCode", "StudentCode").value("ssh_labs", "") or "[]")
        return v if isinstance(v, list) else []
    except ValueError:
        return []


def save_labs(labs):
    QSettings("StudentCode", "StudentCode").setValue("ssh_labs", json.dumps(labs))


def parse_ssh_command(text):
    """« ssh -p 2222 -J relais user@machine » -> champs du formulaire."""
    try:
        parts = shlex.split(text)
    except ValueError:
        return {}
    if not parts or parts[0] != "ssh":
        return {}
    out, i = {}, 1
    while i < len(parts):
        a = parts[i]
        if a == "-p" and i + 1 < len(parts):
            out["port"] = int(parts[i + 1]) if parts[i + 1].isdigit() else 22
            i += 1
        elif a == "-J" and i + 1 < len(parts):
            j = parts[i + 1]
            if "@" in j:
                out["jump_user"], j = j.split("@", 1)
            if ":" in j:
                j, jp = j.rsplit(":", 1)
                out["jump_port"] = int(jp) if jp.isdigit() else 22
            out["jump_host"] = j
            i += 1
        elif a == "-i" and i + 1 < len(parts):
            out["key"], out["auth"] = parts[i + 1], "key"
            i += 1
        elif not a.startswith("-") and "host" not in out:
            if "@" in a:
                out["user"], a = a.split("@", 1)
            out["host"] = a
        i += 1
    return out


def cleanup_on(lab):
    """Nettoyage à la connexion : actif par défaut, sauf si on l'a décoché dans les options."""
    return bool(lab.get("cleanup")) if lab.get("cleanup_set") else True


def looks_like_host(text):
    return "." in text or ":" in text or text.replace("-", "").isdigit() or text == "localhost"


def quick_parse(text, default_user="ubuntu"):
    """« ubuntu@10.0.0.5 », « 10.0.0.5 », « ubuntu@10.0.0.5:2222 » ou une commande ssh
    complète -> (utilisateur, machine, port). L'ordre inversé (ip@ubuntu) est toléré."""
    text = text.strip()
    name = ""
    m = re.match(r"^([^=@]+?)\s*=\s*(.+)$", text)     # « Serveur = ubuntu@10.11.0.2 » : un nom devant
    if m:
        name, text = m.group(1).strip(), m.group(2).strip()
    if text.startswith("ssh "):
        d = parse_ssh_command(text)
        if name:
            d["name"] = name
        return (d.get("user") or default_user, d.get("host", ""), int(d.get("port", 22)), d)
    user, host, port = default_user, text, 22
    if "@" in text:
        a, b = text.split("@", 1)
        user, host = (b, a) if looks_like_host(a) and not looks_like_host(b) else (a, b)
    if host.count(":") == 1 and host.rsplit(":", 1)[1].isdigit():
        host, p = host.rsplit(":", 1)
        port = int(p)
    return user.strip(), host.strip(), port, ({"name": name} if name else {})


def lab_label(lab):
    """Texte affiché pour une machine : son nom, suivi de l'adresse si le nom est personnalisé."""
    return lab["name"] if "@" in lab["name"] else "%s — %s" % (lab["name"], lab["host"])


class LabDialog(QDialog):
    """Création / modification d'un lab (une machine à laquelle on se connecte)."""

    def __init__(self, parent, lab=None, vpns=()):
        super().__init__(parent)
        self.setWindowTitle("Lab SSH")
        self.setMinimumWidth(520)
        lab = lab or {}
        self.lab_id = lab.get("id") or uuid.uuid4().hex[:10]
        f = QFormLayout()
        self.name = QLineEdit(lab.get("name", ""))
        self.name.setPlaceholderText("ex : INFO0506 — VM serveur")
        self.host = QLineEdit(lab.get("host", ""))
        self.host.setPlaceholderText("adresse ou IP — tu peux aussi coller « ssh -p 2222 user@machine »")
        self.host.editingFinished.connect(self.maybe_parse)
        self.port = QSpinBox()
        self.port.setRange(1, 65535)
        self.port.setValue(int(lab.get("port", 22)))
        self.user = QLineEdit(lab.get("user", ""))
        self.auth = QComboBox()
        self.auth.addItems(["Mot de passe", "Clé SSH"])
        self.auth.setCurrentIndex(1 if lab.get("auth") == "key" else 0)
        self.key = QLineEdit(lab.get("key", ""))
        self.key.setPlaceholderText("~/.ssh/id_ed25519 (vide = clés par défaut)")
        kb = QPushButton("…")
        kb.setFixedWidth(34)
        kb.clicked.connect(self.pick_key)
        krow = QHBoxLayout()
        krow.addWidget(self.key)
        krow.addWidget(kb)
        self.dir = QLineEdit(lab.get("dir", ""))
        self.dir.setPlaceholderText("dossier ouvert à la connexion (vide = dossier personnel)")
        self.python = QLineEdit(lab.get("python", "python3"))
        self.cleanup = QCheckBox("À la connexion, fermer ce qui traîne (anciennes sessions, serveurs, java…)")
        self.cleanup.setChecked(cleanup_on(lab))
        warn = QLabel("Ferme tes anciennes sessions SSH, tes programmes en écoute sur un port et tes programmes "
                      "java/python/node restés en arrière-plan, jamais les services du système. Décoche si le "
                      "compte est partagé avec d'autres étudiants, ou si tu laisses volontairement un "
                      "programme tourner.")
        warn.setWordWrap(True)
        warn.setStyleSheet(f"color: {COMMENT};")
        self.vpn = QComboBox()
        self.vpn.addItem("(aucun)", "")
        for n in dict.fromkeys(list(vpns) + ([lab["vpn"]] if lab.get("vpn") else [])):
            self.vpn.addItem(n, n)
        self.vpn.setCurrentIndex(max(self.vpn.findData(lab.get("vpn", "")), 0))
        self.vpn.setToolTip("VPN (onglet VPN) activé automatiquement avant la connexion à ce lab")
        self.jhost = QLineEdit(lab.get("jump_host", ""))
        self.jhost.setPlaceholderText("vide = connexion directe")
        self.jport = QSpinBox()
        self.jport.setRange(1, 65535)
        self.jport.setValue(int(lab.get("jump_port", 22)))
        self.juser = QLineEdit(lab.get("jump_user", ""))
        self.juser.setPlaceholderText("vide = même utilisateur")
        f.addRow("Nom du lab", self.name)
        f.addRow("Machine", self.host)
        f.addRow("Port", self.port)
        f.addRow("Utilisateur", self.user)
        f.addRow("Authentification", self.auth)
        f.addRow("Fichier de clé", krow)
        f.addRow("VPN à activer avant", self.vpn)
        f.addRow("Dossier de départ", self.dir)
        f.addRow("Commande Python", self.python)
        f.addRow("", self.cleanup)
        f.addRow("", warn)
        sep = QLabel("Machine relais (optionnel, équivalent de ssh -J)")
        sep.setStyleSheet(f"color: {COMMENT}; font-weight: 700; padding-top: 8px;")
        f.addRow(sep)
        f.addRow("Relais", self.jhost)
        f.addRow("Port du relais", self.jport)
        f.addRow("Utilisateur du relais", self.juser)
        bb = QDialogButtonBox(QDialogButtonBox.StandardButton.Save | QDialogButtonBox.StandardButton.Cancel)
        bb.button(QDialogButtonBox.StandardButton.Save).setText("Enregistrer")
        bb.button(QDialogButtonBox.StandardButton.Cancel).setText("Annuler")
        bb.accepted.connect(self.check)
        bb.rejected.connect(self.reject)
        lay = QVBoxLayout(self)
        lay.addLayout(f)
        lay.addWidget(bb)
        self.auth.currentIndexChanged.connect(lambda i: self.key.setEnabled(i == 1))
        self.key.setEnabled(self.auth.currentIndex() == 1)

    def pick_key(self):
        p, _ = QFileDialog.getOpenFileName(self, "Clé privée", os.path.expanduser("~/.ssh"))
        if p:
            self.key.setText(p)

    def maybe_parse(self):
        d = parse_ssh_command(self.host.text().strip())
        if not d:
            return
        self.host.setText(d.get("host", ""))
        if "port" in d:
            self.port.setValue(d["port"])
        if "user" in d:
            self.user.setText(d["user"])
        if d.get("auth") == "key":
            self.auth.setCurrentIndex(1)
            self.key.setText(d.get("key", ""))
        if "jump_host" in d:
            self.jhost.setText(d["jump_host"])
            self.jport.setValue(d.get("jump_port", 22))
            self.juser.setText(d.get("jump_user", ""))

    def check(self):
        self.maybe_parse()
        if not self.host.text().strip() or not self.user.text().strip():
            QMessageBox.warning(self, "Lab incomplet", "Il faut au moins la machine et l'utilisateur.")
            return
        self.accept()

    def value(self):
        host = self.host.text().strip()
        return {"id": self.lab_id, "name": self.name.text().strip() or host, "host": host,
                "port": self.port.value(), "user": self.user.text().strip(),
                "auth": "key" if self.auth.currentIndex() == 1 else "password",
                "key": self.key.text().strip(), "dir": self.dir.text().strip(),
                "python": self.python.text().strip() or "python3",
                "vpn": self.vpn.currentData() or "",
                "cleanup": self.cleanup.isChecked(), "cleanup_set": True, "jump_host": self.jhost.text().strip(),
                "jump_port": self.jport.value(), "jump_user": self.juser.text().strip()}


class PasswordDialog(QDialog):
    def __init__(self, parent, lab, why=""):
        super().__init__(parent)
        self.setWindowTitle("Connexion SSH")
        self.setMinimumWidth(420)
        what = "Phrase secrète de la clé" if lab.get("auth") == "key" else "Mot de passe"
        lab_txt = QLabel(f"{what} pour <b>{lab['user']}@{lab['host']}</b>")
        self.pw = QLineEdit()
        self.pw.setEchoMode(QLineEdit.EchoMode.Password)
        self.pw.setPlaceholderText("mot de passe, puis Entrée")
        self.save = QCheckBox("Se souvenir du mot de passe")
        ok = pw_available()
        self.save.setChecked(ok)
        self.save.setEnabled(ok)
        if not ok:
            self.save.setToolTip("Aucun trousseau disponible : le mot de passe sera redemandé à chaque fois.")
        else:
            self.save.setToolTip("Gardé dans le trousseau du système.")
        bb = QDialogButtonBox(QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel)
        bb.button(QDialogButtonBox.StandardButton.Ok).setText("Se connecter")
        bb.button(QDialogButtonBox.StandardButton.Cancel).setText("Annuler")
        bb.accepted.connect(self.accept)
        bb.rejected.connect(self.reject)
        lay = QVBoxLayout(self)
        if why:
            w = QLabel(why)
            w.setStyleSheet(f"color: {YELLOW};")
            w.setWordWrap(True)
            lay.addWidget(w)
        lay.addWidget(lab_txt)
        lay.addWidget(self.pw)
        lay.addWidget(self.save)
        lay.addWidget(bb)


# ------------------------------------------------------- panneau latéral
class RemoteTree(QTreeWidget):
    """Explorateur distant : sélection multiple, et glisser-déposer pour déplacer des fichiers
    sur la machine (ou pour y envoyer des fichiers glissés depuis l'explorateur local)."""

    def __init__(self, panel):
        super().__init__()
        self.panel = panel
        self.setSelectionMode(QTreeWidget.SelectionMode.ExtendedSelection)
        self.setDragEnabled(True)
        self.setAcceptDrops(True)
        self.setDropIndicatorShown(True)
        self.setDragDropMode(QTreeWidget.DragDropMode.DragDrop)
        self.setAutoExpandDelay(600)

    def dest_dir(self, pos):
        p = self.panel
        it = self.itemAt(pos)
        if it is None or not it.data(0, p.ROLE_PATH):
            return p.root
        path = it.data(0, p.ROLE_PATH)
        return path if it.data(0, p.ROLE_DIR) else posixpath.dirname(path)

    def dragEnterEvent(self, e):
        if e.source() is self or e.mimeData().hasUrls():
            e.acceptProposedAction()

    def dragMoveEvent(self, e):
        if not (e.source() is self or e.mimeData().hasUrls()):
            return e.ignore()
        super().dragMoveEvent(e)   # surlignage de la cible, défilement, ouverture des dossiers
        e.acceptProposedAction()

    def dropEvent(self, e):
        dest = self.dest_dir(e.position().toPoint())
        p = self.panel
        if e.source() is self:
            paths = [it.data(0, p.ROLE_PATH) for it in self.selectedItems() if it.data(0, p.ROLE_PATH)]
            paths = [x for x in paths if not any(x.startswith(y + "/") for y in paths)]
            e.setDropAction(Qt.DropAction.IgnoreAction)   # c'est nous qui déplaçons, pas la vue
            e.accept()
            p.move_remote(paths, dest)
        elif e.mimeData().hasUrls():
            files = [u.toLocalFile() for u in e.mimeData().urls() if u.isLocalFile() and os.path.isfile(u.toLocalFile())]
            e.setDropAction(Qt.DropAction.CopyAction)
            e.accept()
            if files:
                p.send_files(files, dest)


class RemotePanel(QWidget):
    """Onglet « SSH » du panneau de gauche : choix du lab, connexion, explorateur distant."""
    ROLE_PATH, ROLE_DIR, ROLE_LOADED, ROLE_SIZE = (Qt.ItemDataRole.UserRole + i for i in range(4))

    def __init__(self, win, console, hub=None):
        super().__init__()
        self.win = win
        self.hub = hub
        self.console = console  # terminal de cette connexion, dans le panneau du bas
        self.ssh = SSH()
        self.lab = None          # lab connecté
        self.busy = False
        self.root = "/"
        self.pending_pw = None
        self.show_hidden = False
        lay = QVBoxLayout(self)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.setSpacing(0)
        from sysinfo import FROZEN
        if not HAS_SSH and FROZEN:
            msg = QLabel("Le module SSH manque dans cette version de SCode : réinstalle-la.")
            msg.setWordWrap(True)
            msg.setStyleSheet("padding: 10px;")
            lay.addWidget(msg)
            lay.addStretch(1)
            return
        if not HAS_SSH:
            msg = QLabel("Le module SSH (paramiko) n'est pas installé.")
            msg.setWordWrap(True)
            msg.setStyleSheet("padding: 10px;")
            b = QPushButton("Installer le module SSH")
            b.clicked.connect(self.install_module)
            lay.addWidget(msg)
            lay.addWidget(b)
            lay.addStretch(1)
            self.install_btn = b
            return

        def tool(name, tip, fn, col=FG):
            b = QToolButton()
            b.setIcon(icon(name, col))
            b.setToolTip(tip)
            b.clicked.connect(fn)
            return b
        self.combo = QComboBox()
        self.combo.setEditable(True)
        self.combo.setInsertPolicy(QComboBox.InsertPolicy.NoInsert)
        self.combo.lineEdit().setPlaceholderText("Serveur = ubuntu@adresse-ip")
        self.combo.setToolTip("Tape utilisateur@machine (ex : ubuntu@10.0.3.7) puis Entrée.\n"
                              "Pour lui donner un nom : Serveur = ubuntu@10.0.3.7\n"
                              "Les machines déjà utilisées restent dans la liste.")
        self.combo.lineEdit().returnPressed.connect(self.connect)
        self.btn = tool("plug", "Se connecter", self.toggle, GREEN)
        more = QToolButton()
        more.setIcon(icon("more", FG))
        more.setToolTip("Gérer les labs")
        more.setPopupMode(QToolButton.ToolButtonPopupMode.InstantPopup)
        more.setStyleSheet("QToolButton::menu-indicator { image: none; }")
        m = QMenu(more)
        from PyQt6.QtGui import QAction
        self.a_rename = QAction(icon("edit", GREEN), "Renommer cette machine…", self)
        self.a_rename.triggered.connect(self.rename_lab)
        m.addAction(icon("edit", BLUE), "Options avancées (port, clé, VPN, relais…)", self.edit_lab)
        m.addAction(icon("trash", RED), "Retirer cette machine de la liste", self.delete_lab)
        m.addSeparator()
        m.insertAction(m.actions()[0], self.a_rename)
        m.addAction(icon("key", YELLOW), "Oublier le mot de passe enregistré", self.forget_pw)
        m.addAction(icon("terminal", GREEN), "Nouveau terminal SSH (dans SCode)", lambda: self.new_terminal())
        m.addAction(icon("terminal", GREEN), "Ouvrir un terminal système sur ce lab", self.external_terminal)
        m.addSeparator()
        self.a_hidden = m.addAction("Afficher les fichiers cachés")
        self.a_hidden.setCheckable(True)
        self.a_hidden.toggled.connect(self.set_hidden)
        more.setMenu(m)
        top = QHBoxLayout()
        top.setContentsMargins(6, 6, 6, 2)
        top.setSpacing(2)
        top.addWidget(self.combo, 1)
        top.addWidget(self.btn)
        top.addWidget(more)
        self.path = QLineEdit()
        self.path.setPlaceholderText("dossier distant")
        self.path.returnPressed.connect(lambda: self.set_root(self.path.text().strip() or "/"))
        nav = QHBoxLayout()
        nav.setContentsMargins(6, 2, 6, 4)
        nav.setSpacing(2)
        nav.addWidget(tool("up", "Dossier parent", lambda: self.set_root(posixpath.dirname(self.root) or "/")))
        nav.addWidget(self.path, 1)
        nav.addWidget(tool("reload", "Actualiser", lambda: self.set_root(self.root)))
        self.tree = RemoteTree(self)
        self.tree.setHeaderHidden(True)
        self.tree.setIndentation(14)
        self.tree.itemExpanded.connect(self.expand)
        self.tree.itemActivated.connect(self.activate)
        self.tree.itemClicked.connect(self.activate)
        self.tree.setContextMenuPolicy(Qt.ContextMenuPolicy.CustomContextMenu)
        self.tree.customContextMenuRequested.connect(self.menu)
        self.status = QLabel()
        self.status.setWordWrap(True)
        self.status.setStyleSheet("padding: 5px 8px;")
        lay.addLayout(top)
        lay.addLayout(nav)
        lay.addWidget(self.tree, 1)
        lay.addWidget(self.status)
        self.ssh.lost.connect(self.on_lost)
        self.ssh.shell_raw.connect(self.console.feed)
        # dès que le shell distant est prêt, on lui donne la taille réelle du terminal
        self.ssh.shell_ready.connect(lambda: self.ssh.shell_resize(self.console.cols, self.console.rows))
        self.ssh.run_out.connect(lambda t: win.out.write(t))
        self.ssh.run_end.connect(self.run_finished)
        self.console.send.connect(self.console_send)
        self.console.resized.connect(self.ssh.shell_resize)
        self.extras = []   # terminaux SSH ouverts en plus sur cette machine
        self.watch = QTimer(self)  # détecte une connexion tombée (lab arrêté, wifi coupé…)
        self.watch.setInterval(4000)
        self.watch.timeout.connect(self.check_alive)
        self.watch.start()
        self.reload_labs()
        self.set_status("Non connecté", COMMENT)
        self.set_enabled(False)

    # ------------------------------------------------------------ divers
    def install_module(self):
        from sysinfo import PYTHON_MISSING, python_cmd
        py = python_cmd()
        if py is None:
            return self.install_btn.setText(PYTHON_MISSING)
        self.install_btn.setEnabled(False)
        self.install_btn.setText("Installation en cours…")
        p = QProcess(self)
        p.finished.connect(lambda *_: self.install_btn.setText(
            "Installé : relance SCode pour activer SSH"))
        p.start(py[0], py[1:] + ["-m", "pip", "install", "paramiko", "keyring"])

    def set_status(self, text, col=FG):
        self.status.setText(text)
        self.status.setStyleSheet(f"padding: 5px 8px; color: {col};")

    def log(self, text, col=COMMENT):
        self.console.say(text, col)

    def set_enabled(self, on):
        self.tree.setEnabled(on)
        self.path.setEnabled(on)

    def connected(self):
        return HAS_SSH and self.lab is not None and self.ssh.is_connected()

    def labs(self):
        return load_labs()

    def current(self, create=False):
        """La machine désignée par le champ : une machine connue, ou (si `create`)
        une nouvelle, créée à partir de « utilisateur@machine »."""
        text = self.combo.currentText().strip()
        labs = self.labs()
        for l in labs:
            if text in (l["name"], lab_label(l), "%s@%s" % (l["user"], l["host"])):
                return l
        if not text or not create:
            return None
        last = next((l["user"] for l in labs if l["id"] == QSettings("StudentCode", "StudentCode").value("ssh_last", "")), "ubuntu")
        user, host, port, extra = quick_parse(text, last)
        if not host or " " in host:
            self.set_status("Écris l'adresse sous la forme ubuntu@adresse-ip", RED)
            return None
        for l in labs:  # même machine déjà enregistrée (un nom donné devant la renomme)
            if (l["user"], l["host"], int(l["port"])) == (user, host, port):
                if extra.get("name") and extra["name"] != l["name"]:
                    l["name"] = extra["name"]
                    save_labs(labs)
                    self.reload_labs(l["id"])
                return l
        name = extra.get("name") or "%s@%s" % (user, host) + ("" if port == 22 else ":%d" % port)
        lab = {"id": uuid.uuid4().hex[:10], "name": name, "host": host, "port": port, "user": user,
               "auth": "password", "key": "", "dir": "", "python": "python3", "cleanup": False,
               "vpn": "", "jump_host": "", "jump_port": 22, "jump_user": ""}
        lab.update({k: v for k, v in extra.items() if k in ("auth", "key", "jump_host", "jump_port", "jump_user")})
        save_labs(labs + [lab])
        self.reload_labs(lab["id"])
        return lab

    def reload_labs(self, select=None):
        select = select or self.combo.currentData() or QSettings("StudentCode", "StudentCode").value("ssh_last", "")
        self.combo.blockSignals(True)
        self.combo.clear()
        for l in self.labs():
            self.combo.addItem(icon("server", CYAN), lab_label(l), l["id"])
            self.combo.setItemData(self.combo.count() - 1, "%s@%s:%s" % (l["user"], l["host"], l["port"]),
                                   Qt.ItemDataRole.ToolTipRole)
        i = self.combo.findData(select)
        self.combo.setCurrentIndex(i if i >= 0 else (0 if self.combo.count() else -1))
        if not self.combo.count():
            self.combo.setEditText("")
        self.combo.blockSignals(False)

    def set_hidden(self, on):
        self.show_hidden = on
        if self.connected():
            self.set_root(self.root)

    def vpn_names(self):
        vp = getattr(self.win, "vpn_panel", None)
        return vp.names() if vp else []

    # -------------------------------------------------------------- labs
    def new_lab(self):
        d = LabDialog(self, vpns=self.vpn_names())
        if d.exec():
            lab = d.value()
            save_labs(self.labs() + [lab])
            self.reload_labs(lab["id"])
            return lab
        return None

    def edit_lab(self):
        lab = self.current()
        if not lab:
            return self.new_lab()
        d = LabDialog(self, lab, vpns=self.vpn_names())
        if d.exec():
            new = d.value()
            save_labs([new if l["id"] == lab["id"] else l for l in self.labs()])
            self.reload_labs(new["id"])

    def delete_lab(self):
        lab = self.current()
        if lab and QMessageBox.question(self, "Supprimer", f"Supprimer le lab « {lab['name']} » ?") \
                == QMessageBox.StandardButton.Yes:
            if self.lab and self.lab["id"] == lab["id"]:
                self.disconnect()
            pw_del(lab["id"])
            save_labs([l for l in self.labs() if l["id"] != lab["id"]])
            self.reload_labs()

    def rename_lab(self):
        lab = self.current()
        if not lab:
            return
        name, ok = QInputDialog.getText(self, "Renommer la machine",
                                        f"Nom pour {lab['user']}@{lab['host']} (ex : Serveur, Croupier, Clients) :",
                                        text="" if "@" in lab["name"] else lab["name"])
        name = name.strip()
        if not ok or not name or "=" in name:
            return
        labs = self.labs()
        for l in labs:
            if l["id"] == lab["id"]:
                l["name"] = name
        save_labs(labs)
        if self.lab and self.lab["id"] == lab["id"]:
            self.lab["name"] = name
            self.connected_status()
            for ed in self.win.editors():   # les onglets de ses fichiers prennent le nouveau nom
                if ed.remote_lab == lab["id"]:
                    ed.remote_name = name
                    self.win.retitle(ed)
        if self.hub:
            self.hub.labs_changed(lab["id"])
        else:
            self.reload_labs(lab["id"])
        if self.hub:
            self.hub.retitle(self)

    def forget_pw(self):
        lab = self.current()
        if lab:
            pw_del(lab["id"])
            self.win.statusBar().showMessage("Mot de passe oublié pour ce lab.", 3000)

    def external_terminal(self):
        lab = self.current()
        if not lab:
            return
        cmd = ["ssh", "-p", str(lab["port"])]
        if lab.get("jump_host"):
            j = lab["jump_host"]
            if lab.get("jump_user"):
                j = lab["jump_user"] + "@" + j
            cmd += ["-J", "%s:%s" % (j, lab.get("jump_port", 22))]
        if lab.get("auth") == "key" and lab.get("key"):
            cmd += ["-i", os.path.expanduser(lab["key"])]
        cmd.append("%s@%s" % (lab["user"], lab["host"]))
        from sysinfo import WINDOWS
        if WINDOWS:   # ssh.exe est fourni avec Windows 10 et 11
            wt = shutil.which("wt")
            args = [wt, "new-tab"] + cmd if wt else ["cmd", "/c", "start", "", "cmd", "/k"] + cmd
            subprocess.Popen(args, creationflags=getattr(subprocess, "CREATE_NEW_CONSOLE", 0) if not wt else 0)
            return
        for term, flag in (("ptyxis", "--"), ("gnome-terminal", "--"), ("kgx", "-e"),
                           ("konsole", "-e"), ("xfce4-terminal", "-x"), ("xterm", "-e"),
                           ("x-terminal-emulator", "-e")):
            if shutil.which(term):
                subprocess.Popen([term, flag] + cmd, start_new_session=True)
                return
        QMessageBox.information(self, "Terminal", "Aucun terminal trouvé. Commande :\n" + shlex.join(cmd))

    # --------------------------------------------------------- connexion
    def toggle(self):
        if self.busy:
            return
        if self.connected() or self.lab is not None:
            self.disconnect()
        else:
            self.connect()

    def connect(self, ask=False, why=""):
        if not HAS_SSH or self.busy:
            return
        lab = self.current(create=True)
        if not lab:
            return
        password = None
        save = False
        if lab.get("auth") != "key" or ask:
            password = None if ask else pw_get(lab["id"])
            if password is None:
                d = PasswordDialog(self, lab, why)
                if not d.exec():
                    return
                password, save = d.pw.text(), d.save.isChecked()
        elif lab.get("auth") == "key":
            password = pw_get(lab["id"])  # phrase secrète éventuelle
        self.pending_pw = (lab["id"], password) if save else None
        vp = getattr(self.win, "vpn_panel", None)
        # VPN du lab, ou à défaut le VPN déjà actif : la machine doit passer par lui
        vpn_name = lab.get("vpn") or (vp.active_name() if vp is not None else "")
        if vpn_name and vp is not None:
            self.busy = True
            self.btn.setEnabled(False)
            self.set_status(f"VPN {vpn_name}…", YELLOW)

            def vpn_failed(msg):
                self.busy = False
                self.btn.setEnabled(True)
                self.pending_pw = None
                self.set_status("VPN : " + msg.splitlines()[0], RED)
                self.log("VPN impossible à activer : " + msg, RED)
            vp.ensure_up(vpn_name, lambda: self.start_connect(lab, password), vpn_failed, host=lab["host"])
            return
        self.start_connect(lab, password)

    def start_connect(self, lab, password):
        self.busy = True
        self.btn.setEnabled(False)
        self.set_status(f"Connexion à {lab['name']}…", YELLOW)
        self._try = (lab, password)
        self.ssh.submit(lambda: self.ssh.connect_(lab, password),
                        ok=lambda info: self.on_connected(lab, info), err=self.on_connect_error)

    def on_connect_error(self, e):
        self.busy = False
        self.btn.setEnabled(True)
        lab, password = self._try
        self.set_status("Non connecté", COMMENT)
        if isinstance(e, UnknownHost):
            # première connexion : on retient la clé de la machine (comme « yes » avec ssh) ;
            # c'est un changement de clé par la suite qui déclenche un avertissement
            trust_host(e.name, e.key)
            self.first_seen = (e.name, fingerprint(e.key))
            self.start_connect(lab, password)
            return
        if isinstance(e, paramiko.BadHostKeyException):
            r = QMessageBox.warning(
                self, "La clé de la machine a changé",
                f"La machine {e.hostname} ne présente plus la même clé qu'avant.\n"
                f"Nouvelle empreinte : {fingerprint(e.key)}\n\n"
                "C'est normal si le lab a été recréé ou réinitialisé. Sinon, quelqu'un pourrait "
                "se faire passer pour la machine.\n\nFaire confiance à la nouvelle clé ?",
                QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No,
                QMessageBox.StandardButton.No)
            if r == QMessageBox.StandardButton.Yes:
                trust_host(e.hostname, e.key)
                self.start_connect(lab, password)
            return
        if isinstance(e, paramiko.BadAuthenticationType) and lab.get("auth") != "key" \
                and "password" not in (e.allowed_types or []):
            # la machine n'accepte pas les mots de passe en SSH : inutile de le redemander
            self.pending_pw = None
            self.set_status("La machine refuse les mots de passe en SSH (clé uniquement).", RED)
            self.log("Connexion refusée : cette machine n'accepte que les clés SSH, pas les mots de passe.", RED)
            self.log("Pour autoriser le mot de passe, dans la console de la machine (RemoteLabz) :", COMMENT)
            self.log("    " + ENABLE_PASSWORD, FG)
            self.log("puis reconnecte-toi. (Ou utilise une clé : menu « … » → Options avancées.)", COMMENT)
            self.win.bottom.setCurrentWidget(self.console)
            return
        if isinstance(e, paramiko.AuthenticationException) or \
                (isinstance(e, paramiko.SSHException) and "passphrase" in str(e).lower()) or \
                isinstance(e, getattr(paramiko, "PasswordRequiredException", ())):
            pw_del(lab["id"])
            self.pending_pw = None
            self.connect(ask=True, why="Mot de passe refusé — réessaie." if lab.get("auth") != "key"
                         else "La clé demande une phrase secrète (ou a été refusée).")
            return
        self.pending_pw = None
        self.set_status("Échec : " + friendly(e) + "  Recherche de la cause…", RED)
        self.log("Connexion impossible : " + friendly(e), RED)
        if lab.get("jump_host"):
            return

        def found(res):
            if self.busy or self.lab is not None:
                return
            short, detail = res
            self.set_status("Échec : " + short + " Détails dans « Terminal SSH ».", RED)
            self.log("Diagnostic : " + short, YELLOW)
            self.log(detail, FG)
            self.win.bottom.show()
            self.win.bottom.setCurrentWidget(self.console)
        self.ssh.submit(lambda: diagnose(lab["host"], lab["port"]), ok=found,
                        err=lambda _e: self.set_status("Échec : " + friendly(e), RED))

    def on_connected(self, lab, info):
        self.busy = False
        self.btn.setEnabled(True)
        self.lab = lab
        if self.pending_pw and self.pending_pw[0] == lab["id"] and self.pending_pw[1] is not None:
            if not pw_set(*self.pending_pw):
                self.win.statusBar().showMessage("Le mot de passe n'a pas pu être enregistré.", 5000)
        self.pending_pw = None
        QSettings("StudentCode", "StudentCode").setValue("ssh_last", lab["id"])
        if self.hub:
            self.hub.retitle(self)
        self.btn.setIcon(icon("plug", RED))
        self.btn.setToolTip("Se déconnecter")
        self.combo.setEnabled(False)
        self.set_enabled(True)
        self.connected_status()
        self.console.clear()
        self.log(f"── Connecté à {lab['name']} ({lab['user']}@{lab['host']}:{lab['port']}) ──", GREEN)
        if getattr(self, "first_seen", None):
            self.log("Première connexion à %s : clé de la machine retenue (%s)." % self.first_seen, COMMENT)
            self.first_seen = None
        if cleanup_on(lab):
            n = len(info["cleaned"])
            self.log(f"Nettoyage : {n} processus fermé(s) (anciennes sessions, serveurs, programmes en "
                     "arrière-plan)." if n else "Nettoyage : rien à fermer.", COMMENT)
            for ln in info["cleaned"][:30]:
                self.log("   " + ln, COMMENT)
        self.home = info["home"]
        self.set_root(lab.get("dir") or self.home)
        self.open_shell()

    def disconnect(self, msg="Déconnecté"):
        self.close_extras()
        if self.win.rrun is self:
            self.ssh.run_stop()
        self.ssh.submit(self.ssh.close)
        self.lab = None
        self.busy = False
        self.btn.setEnabled(True)
        self.btn.setIcon(icon("plug", GREEN))
        self.btn.setToolTip("Se connecter")
        self.combo.setEnabled(True)
        if self.hub:
            self.hub.retitle(self)
        self.tree.clear()
        self.set_enabled(False)
        self.set_status(msg, COMMENT)

    def on_lost(self, why):
        if self.lab is None:
            return
        lab = self.lab
        self.disconnect("Connexion perdue : " + why)
        self.set_status("Connexion perdue — clique sur la prise pour te reconnecter.", RED)
        self.log(f"── Connexion à {lab['name']} perdue : {why} ──", RED)

    def check_alive(self):
        if self.lab is not None and not self.busy and not self.ssh.is_connected():
            self.on_lost("la machine ne répond plus")

    def fail(self, e):
        """Erreur d'une opération distante."""
        if self.lab is not None and not self.ssh.is_connected():
            return  # « connexion perdue » est déjà signalé
        self.win.statusBar().showMessage("SSH : " + friendly(e), 8000)

    # ------------------------------------------------------- explorateur
    def set_root(self, path):
        if not self.connected():
            return
        path = posixpath.normpath(path) if path.startswith("/") else posixpath.normpath(
            posixpath.join(self.root, path))

        def ok(entries):
            self.root = path
            self.path.setText(path)
            self.tree.clear()
            self.fill(self.tree.invisibleRootItem(), path, entries)
        self.ssh.submit(lambda: self.listing(path), ok=ok, err=self.fail)

    def listing(self, path):
        out = []
        for a in self.ssh.sftp.listdir_attr(path):
            is_dir = stat.S_ISDIR(a.st_mode or 0)
            if stat.S_ISLNK(a.st_mode or 0):
                try:
                    is_dir = stat.S_ISDIR(self.ssh.sftp.stat(posixpath.join(path, a.filename)).st_mode)
                except OSError:
                    pass
            out.append((a.filename, is_dir, a.st_size or 0))
        out.sort(key=lambda t: (not t[1], t[0].lower()))
        return out

    def fill(self, parent, path, entries):
        for name, is_dir, size in entries:
            if name.startswith(".") and not self.show_hidden:
                continue
            it = QTreeWidgetItem([name])
            it.setIcon(0, icon("folder", BLUE) if is_dir else file_icon(name))
            it.setData(0, self.ROLE_PATH, posixpath.join(path, name))
            it.setData(0, self.ROLE_DIR, is_dir)
            it.setData(0, self.ROLE_SIZE, size)
            if is_dir:
                it.addChild(QTreeWidgetItem(["…"]))
            parent.addChild(it)

    def expand(self, it):
        if it.data(0, self.ROLE_LOADED) or not it.data(0, self.ROLE_DIR):
            return
        it.setData(0, self.ROLE_LOADED, True)
        path = it.data(0, self.ROLE_PATH)

        def ok(entries):
            it.takeChildren()
            self.fill(it, path, entries)
        self.ssh.submit(lambda: self.listing(path), ok=ok, err=self.fail)

    def refresh_dir(self, path):
        """Recharge le dossier `path` s'il est affiché."""
        if path == self.root:
            return self.set_root(self.root)
        stack = [self.tree.topLevelItem(i) for i in range(self.tree.topLevelItemCount())]
        while stack:
            it = stack.pop()
            if it.data(0, self.ROLE_PATH) == path:
                it.setData(0, self.ROLE_LOADED, False)
                if it.isExpanded():
                    self.expand(it)
                return
            stack.extend(it.child(i) for i in range(it.childCount()))

    def move_remote(self, paths, dest):
        """Déplace des fichiers ou dossiers vers `dest` sur la machine."""
        if not self.connected():
            return
        todo, skipped = [], []
        for src in paths:
            name = posixpath.basename(src)
            if posixpath.dirname(src) == dest:
                continue
            if dest == src or dest.startswith(src + "/"):
                skipped.append(f"« {name} » : impossible de déplacer un dossier dans lui-même")
                continue
            todo.append((src, posixpath.join(dest, name)))
        if not todo:
            if skipped:
                self.win.statusBar().showMessage(skipped[0], 6000)
            return

        def work():
            done, errors = [], list(skipped)
            for src, target in todo:
                try:
                    self.ssh.sftp.stat(target)
                    errors.append(f"« {posixpath.basename(src)} » existe déjà dans la destination")
                    continue
                except OSError:
                    pass
                try:
                    self.ssh.sftp.rename(src, target)
                    done.append((src, target))
                except OSError as e:
                    errors.append(f"« {posixpath.basename(src)} » : {friendly(e)}")
            return done, errors

        def finished(res):
            done, errors = res
            for src, target in done:   # les onglets ouverts suivent le fichier déplacé
                for ed in self.win.editors():
                    if ed.remote_lab == self.lab["id"] and ed.remote and \
                            (ed.remote == src or ed.remote.startswith(src + "/")):
                        ed.remote = target + ed.remote[len(src):]
                        self.win.retitle(ed)
            for d in {posixpath.dirname(src) for src, _ in done} | ({dest} if done else set()):
                self.refresh_dir(d)
            self.win.update_status()
            if done:
                self.win.statusBar().showMessage(
                    f"{len(done)} élément(s) déplacé(s) vers {posixpath.basename(dest) or dest}", 4000)
            if errors:
                QMessageBox.warning(self, "Déplacement", "\n".join(errors))
        self.ssh.submit(work, ok=finished, err=self.fail)

    def send_files(self, files, dest):
        """Envoie des fichiers locaux dans le dossier distant `dest`."""
        if not self.connected():
            return

        def send():
            for f in files:
                self.ssh.sftp.put(f, posixpath.join(dest, os.path.basename(f)))
            return len(files)
        self.set_status("Envoi en cours…", YELLOW)
        self.ssh.submit(send, ok=lambda n: (self.refresh_dir(dest), self.connected_status(),
                                            self.win.statusBar().showMessage(f"{n} fichier(s) envoyé(s).", 4000)),
                        err=lambda e: (self.connected_status(), self.fail(e)))

    def activate(self, it, _col=0):
        if QApplication.keyboardModifiers() != Qt.KeyboardModifier.NoModifier:
            return
        path = it.data(0, self.ROLE_PATH)
        if not path or it.data(0, self.ROLE_DIR):
            return
        self.open_remote(path, it.data(0, self.ROLE_SIZE) or 0)

    def cache_path(self, rpath):
        return os.path.join(CACHE_DIR, self.lab["id"], rpath.lstrip("/"))

    def open_remote(self, rpath, size=0):
        if not self.connected():
            return
        local = self.cache_path(rpath)
        for ed in self.win.editors():  # déjà ouvert : on y va
            if getattr(ed, "remote", None) == rpath and ed.path == local:
                self.win.open_file(local)
                return
        if size > MAX_EDIT:
            QMessageBox.information(self, "Fichier volumineux",
                                    "Ce fichier dépasse 5 Mo : télécharge-le (clic droit) plutôt que de l'éditer.")
            return

        def read():
            with self.ssh.sftp.open(rpath, "rb") as fh:
                return fh.read()

        def ok(data):
            os.makedirs(os.path.dirname(local), exist_ok=True)
            with open(local, "wb") as fh:
                fh.write(data)
            ed = self.win.open_file(local)
            if ed is not None:
                ed.remote, ed.remote_lab = rpath, self.lab["id"]
                ed.remote_name = "" if "@" in self.lab["name"] else self.lab["name"]
                self.win.retitle(ed)
                self.win.update_status()
            elif os.path.splitext(local)[1].lower() not in (".pdf", ".png", ".jpg", ".jpeg", ".gif",
                                                              ".bmp", ".webp", ".svg"):
                self.win.statusBar().showMessage("Ce fichier n'est pas du texte : utilise Télécharger.", 6000)
        self.set_status(f"Ouverture de {posixpath.basename(rpath)}…", YELLOW)
        self.ssh.submit(read, ok=lambda d: (ok(d), self.connected_status()), err=self.fail)

    def connected_status(self):
        if self.lab:
            who = f"{self.lab['user']}@{self.lab['host']}"
            self.set_status(f"● {self.lab['name']} : {who}" if "@" not in self.lab["name"]
                            else f"● Connecté : {who}", GREEN)

    def upload(self, ed, then=None):
        """Envoie le contenu d'un éditeur distant sur la machine (après un enregistrement)."""
        if not self.connected() or ed.remote_lab != self.lab["id"]:
            ed.unsent = True
            self.win.retitle(ed)
            QMessageBox.warning(self, "Non connecté",
                                "Ce fichier vient d'un lab qui n'est pas connecté.\nIl est gardé en local ; "
                                "reconnecte-toi puis enregistre à nouveau pour l'envoyer.")
            return
        data, rpath = ed.text().encode("utf-8"), ed.remote

        def write():
            with self.ssh.sftp.open(rpath, "wb") as fh:
                fh.write(data)

        def ok(_):
            ed.unsent = False
            self.win.retitle(ed)
            self.win.statusBar().showMessage(f"Envoyé sur {self.lab['name']} : {rpath}", 3000)
            if then:
                then()

        def err(e):
            ed.unsent = True
            self.win.retitle(ed)
            QMessageBox.warning(self, "Envoi impossible",
                                f"{rpath} n'a pas pu être enregistré sur la machine :\n{friendly(e)}")
        self.ssh.submit(write, ok=ok, err=err)

    # ------------------------------------------------- exécution distante
    def run_remote(self, ed, debug=False):
        win = self.win
        if debug:
            win.statusBar().showMessage(
                "Le débogage pas à pas fonctionne sur les fichiers locaux ; ici, utilise Exécuter (F5).", 7000)
            return
        if win.rrun or (win.proc and win.state != "post"):
            return
        if win.proc:
            win.kill_proc()
        if not self.connected() or ed.remote_lab != self.lab["id"]:
            QMessageBox.warning(self, "Non connecté", "Connecte-toi au lab de ce fichier pour l'exécuter.")
            return
        rpath = ed.remote
        ext = posixpath.splitext(rpath)[1].lower()
        q = shlex.quote
        base = run_command(rpath, python=f"{self.lab.get('python') or 'python3'} -u", unbuffer=False)
        if base is None:
            win.statusBar().showMessage(
                "Je ne sais pas exécuter ce type de fichier : lance-le depuis le Terminal SSH.", 7000)
            return
        cmd = "stty -echo 2>/dev/null; " + base

        def go():
            win.clear_marks(errors=True)
            win.out.out.clear()
            win.bottom.setCurrentWidget(win.out)
            win.bottom.show()
            win.out.write(f"── Exécution sur {self.lab['name']} : {rpath} ──\n", COMMENT)
            win.rrun = self
            win.set_state("running")
            self.ssh.start_run(cmd)
        win.save(ed, then=go)

    def run_finished(self, code):
        win = self.win
        if win.rrun is not self:
            return
        win.rrun = None
        win.out.write(f"\n── Programme distant terminé (code {code}) ──\n", COMMENT if code == 0 else YELLOW)
        win.set_state("idle")

    # ----------------------------------------------------------- console
    def open_shell(self):
        self.ssh.start_shell(self.console.cols, self.console.rows, self.console.kind)

    # ------------------------------------------------- terminaux en plus
    def new_terminal(self, path=None):
        """Un autre terminal sur cette machine (clic droit → « Nouveau terminal »)."""
        if self.lab is None or self.ssh.client is None:
            self.win.statusBar().showMessage("Connecte-toi d'abord à cette machine.", 5000)
            return None
        from terminal import make_term
        console = make_term(self.win.code_font, "")
        self.n_extra = getattr(self, "n_extra", 0) + 1
        console.setObjectName("ssh_extra_%d_%d" % (id(self), self.n_extra))
        sh = ExtraShell(self.ssh.client, getattr(console, "cols", 200), getattr(console, "rows", 50),
                        getattr(console, "kind", "xterm-256color"), path)
        sh.raw.connect(console.feed)
        sh.ready.connect(lambda: sh.resize(console.cols, console.rows))
        console.send.connect(sh.send)
        console.resized.connect(sh.resize)
        console.ssh_shell = sh       # garde le canal en vie avec l'onglet
        self.extras.append(console)
        b = self.win.bottom
        mine = [b.indexOf(c) for c in [self.console] + self.extras[:-1] if b.indexOf(c) >= 0]
        i = b.insertTab((max(mine) + 1) if mine else b.count(), console, icon("server", GREEN, size=15), "")
        close = QToolButton()
        close.setIcon(icon("close", COMMENT, size=14))
        close.setAutoRaise(True)
        close.setFixedSize(18, 18)
        close.setToolTip("Fermer ce terminal")
        close.setStyleSheet("QToolButton { padding: 0; border-radius: 4px; } QToolButton:hover { background: %s; }" % BG_HL)
        close.clicked.connect(lambda: self.close_terminal(console))
        b.tabBar().setTabButton(i, QTabBar.ButtonPosition.RightSide, close)
        self.retitle_extras()
        b.show()
        b.setCurrentWidget(console)
        console.setFocus()
        return console

    def retitle_extras(self):
        b = self.win.bottom
        name = self.hub.title(self) if self.hub else (self.lab or {}).get("name", "SSH")
        for k, c in enumerate(self.extras):
            j = b.indexOf(c)
            if j >= 0:
                b.setTabText(j, "SSH %s %d  " % (name, k + 2))

    def close_terminal(self, console):
        if console not in self.extras:
            return
        sh = getattr(console, "ssh_shell", None)
        if sh is not None:
            sh.close()
        self.extras.remove(console)
        b = self.win.bottom
        j = b.indexOf(console)
        if j >= 0:
            b.removeTab(j)
        console.deleteLater()
        self.retitle_extras()

    def close_extras(self):
        for c in list(self.extras):
            self.close_terminal(c)

    def console_send(self, data):
        if not self.connected():
            self.win.statusBar().showMessage("Connecte-toi d'abord à un lab (onglet SSH à gauche).", 5000)
            return
        if not self.ssh.shell_send(data):
            self.open_shell()
            self.log("(console relancée — renvoie ta commande)", COMMENT)

    # -------------------------------------------------------------- menu
    def menu(self, pos):
        if not self.connected():
            return
        it = self.tree.itemAt(pos)
        path = it.data(0, self.ROLE_PATH) if it else self.root
        is_dir = it.data(0, self.ROLE_DIR) if it else True
        base = path if is_dir else posixpath.dirname(path)
        m = QMenu(self)
        a_new = m.addAction(icon("new", FG), "Nouveau fichier…")
        a_dir = m.addAction(icon("folder", BLUE), "Nouveau dossier…")
        a_up = m.addAction(icon("upload", GREEN), "Envoyer des fichiers ici…")
        a_term = m.addAction(icon("terminal", GREEN), "Nouveau terminal ici")
        a_ren = a_del = a_dl = a_cd = None
        if it:
            m.addSeparator()
            if is_dir:
                a_cd = m.addAction(icon("open", YELLOW), "Ouvrir comme dossier racine")
            else:
                a_dl = m.addAction(icon("download", CYAN), "Télécharger")
            a_ren = m.addAction(icon("edit", BLUE), "Renommer…")
            a_del = m.addAction(icon("trash", RED), "Supprimer")
        m.addSeparator()
        a_copy = m.addAction("Copier le chemin")
        a_ref = m.addAction(icon("reload", FG), "Actualiser")
        r = m.exec(self.tree.viewport().mapToGlobal(pos))
        sftp = lambda: self.ssh.sftp
        done = lambda d: (lambda _=None: self.refresh_dir(d))
        if r is None:
            return
        if r == a_term:
            self.new_terminal(base)
        elif r == a_ref:
            self.refresh_dir(base)
        elif r == a_copy:
            QApplication.clipboard().setText(path)
        elif r == a_cd:
            self.set_root(path)
        elif r in (a_new, a_dir):
            name, ok = QInputDialog.getText(self, r.text(), "Nom :", text="script.py" if r == a_new else "dossier")
            name = name.strip()
            if not (ok and name) or "/" in name:
                return
            target = posixpath.join(base, name)

            def create():
                try:
                    sftp().stat(target)
                except OSError:
                    pass
                else:
                    raise FileExistsError(f"« {name} » existe déjà.")
                if r == a_dir:
                    sftp().mkdir(target)
                else:
                    sftp().open(target, "wb").close()
            self.ssh.submit(create, ok=lambda _: (self.refresh_dir(base),
                                                  self.open_remote(target) if r == a_new else None),
                            err=self.fail)
        elif r == a_ren:
            name, ok = QInputDialog.getText(self, "Renommer", "Nouveau nom :", text=posixpath.basename(path))
            name = name.strip()
            if not (ok and name) or "/" in name or name == posixpath.basename(path):
                return
            new = posixpath.join(posixpath.dirname(path), name)

            def rename():
                try:
                    sftp().stat(new)
                except OSError:
                    return sftp().rename(path, new)
                raise FileExistsError(f"« {name} » existe déjà.")

            def renamed(_):
                for ed in self.win.editors():
                    rem = getattr(ed, "remote", None)
                    if rem and (rem == path or rem.startswith(path + "/")):
                        ed.remote = new + rem[len(path):]
                self.refresh_dir(posixpath.dirname(path))
            self.ssh.submit(rename, ok=renamed, err=self.fail)
        elif r == a_del:
            what = "le dossier et tout son contenu" if is_dir else "le fichier"
            if QMessageBox.question(self, "Supprimer sur la machine",
                                    f"Supprimer définitivement {what} « {posixpath.basename(path)} » ?\n"
                                    "(Il n'y a pas de corbeille sur la machine distante.)") \
                    != QMessageBox.StandardButton.Yes:
                return

            def delete():
                if is_dir:
                    out, rc = self.ssh.exec_("rm -rf -- " + shlex.quote(path) + " 2>&1", timeout=60)
                    if rc != 0:
                        raise OSError(out.strip() or "suppression refusée")
                else:
                    sftp().remove(path)
            self.ssh.submit(delete, ok=done(posixpath.dirname(path)), err=self.fail)
        elif r == a_up:
            files, _ = QFileDialog.getOpenFileNames(self, "Envoyer sur la machine", self.win.root)
            if not files:
                return

            def send():
                for f in files:
                    sftp().put(f, posixpath.join(base, os.path.basename(f)))
                return len(files)
            self.set_status("Envoi en cours…", YELLOW)
            self.ssh.submit(send, ok=lambda n: (self.refresh_dir(base), self.connected_status(),
                                                self.win.statusBar().showMessage(f"{n} fichier(s) envoyé(s).", 4000)),
                            err=lambda e: (self.connected_status(), self.fail(e)))
        elif r == a_dl:
            from PyQt6.QtCore import QStandardPaths
            d = QStandardPaths.writableLocation(QStandardPaths.StandardLocation.DownloadLocation)
            target, _ = QFileDialog.getSaveFileName(self, "Télécharger", os.path.join(d, posixpath.basename(path)))
            if target:
                self.ssh.submit(lambda: sftp().get(path, target),
                                ok=lambda _: self.win.statusBar().showMessage(f"Téléchargé : {target}", 5000),
                                err=self.fail)

    def shutdown(self):
        if HAS_SSH:
            try:
                self.ssh.close()
            except Exception:
                pass


class RemoteHub(QWidget):
    """Onglet « SSH » du panneau de gauche : plusieurs connexions en même temps, une par
    sous-onglet (Serveur, Croupier, Clients…), chacune avec son explorateur et son terminal."""

    def __init__(self, win):
        super().__init__()
        self.win = win
        self.count = 0
        from PyQt6.QtWidgets import QTabWidget
        self.tabs = QTabWidget()
        self.tabs.setDocumentMode(True)
        self.tabs.setTabsClosable(True)
        self.tabs.setMovable(True)
        self.tabs.setUsesScrollButtons(True)
        self.tabs.setStyleSheet("QTabBar::tab { padding: 5px 8px; }")
        self.tabs.tabCloseRequested.connect(self.close_session)
        self.tabs.currentChanged.connect(self.session_shown)
        self.tabs.tabBar().setContextMenuPolicy(Qt.ContextMenuPolicy.CustomContextMenu)
        self.tabs.tabBar().customContextMenuRequested.connect(self.sub_menu)
        plus = QToolButton()
        plus.setIcon(icon("new", GREEN))
        plus.setToolTip("Nouvelle connexion SSH (une autre machine en même temps)")
        plus.clicked.connect(lambda: self.add_session(focus=True))
        self.tabs.setCornerWidget(plus, Qt.Corner.TopRightCorner)
        lay = QVBoxLayout(self)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.addWidget(self.tabs)
        self.add_session()

    def panels(self):
        return [self.tabs.widget(i) for i in range(self.tabs.count())]

    def panel_of(self, widget):
        """La connexion à laquelle appartient un terminal du panneau du bas."""
        for p in self.panels():
            if widget is p.console or widget in getattr(p, "extras", []):
                return p
        return None

    def sub_menu(self, pos):
        i = self.tabs.tabBar().tabAt(pos)
        panel = self.tabs.widget(i) if i >= 0 else None
        if panel is None or not HAS_SSH or panel.lab is None:
            return
        m = QMenu(self)
        a_new = m.addAction(icon("terminal", GREEN), "Nouveau terminal")
        a_off = m.addAction(icon("plug", RED), "Se déconnecter")
        r = m.exec(self.tabs.tabBar().mapToGlobal(pos))
        if r == a_new:
            panel.new_terminal()
        elif r == a_off:
            panel.disconnect()

    def current_panel(self):
        return self.tabs.currentWidget()

    def add_session(self, focus=False):
        from terminal import make_term
        self.count += 1
        console = make_term(self.win.code_font, "Connecte-toi à une machine (onglet SSH à gauche) : "
                                                "son terminal s'ouvrira ici.")
        console.setObjectName("ssh_term_%d" % self.count)
        panel = RemotePanel(self.win, console, hub=self)
        self.win.bottom.addTab(console, icon("server", COMMENT, size=15), "Terminal SSH  ")
        i = self.tabs.addTab(panel, icon("server", COMMENT, size=15), "Machine %d" % self.count)
        if hasattr(self.win, "apply_scrollbars") and hasattr(self.win, "a"):
            self.win.apply_scrollbars()
        self.tabs.setCurrentIndex(i)
        self.retitle(panel)
        if focus and HAS_SSH:
            panel.combo.setEditText("")
            panel.combo.setFocus()
        return panel

    def title(self, panel):
        lab = panel.lab
        if lab is None:
            return "Nouvelle"
        return lab["name"] if "@" not in lab["name"] else lab["host"]

    def retitle(self, panel):
        i = self.tabs.indexOf(panel)
        if i < 0:
            return
        on = panel.lab is not None
        self.tabs.setTabText(i, self.title(panel))
        self.tabs.setTabIcon(i, icon("server", GREEN if on else COMMENT, size=15))
        self.tabs.setTabToolTip(i, "%s@%s" % (panel.lab["user"], panel.lab["host"]) if on else "non connecté")
        b = self.win.bottom
        j = b.indexOf(panel.console)
        if j >= 0:
            b.setTabText(j, ("SSH " + self.title(panel) if on else "Terminal SSH") + "  ")
            b.setTabIcon(j, icon("server", GREEN if on else COMMENT, size=15))
        panel.retitle_extras()

    def labs_changed(self, select=None):
        """La liste des machines a changé (renommage…) : les autres sous-onglets se mettent à jour."""
        for p in self.panels():
            if HAS_SSH and p.lab is None:
                p.reload_labs(select if p is self.current_panel() else None)
            elif HAS_SSH:
                p.reload_labs(p.lab["id"])

    def session_shown(self, _i):
        p = self.current_panel()
        if p is not None and HAS_SSH and p.lab is None and not p.busy and not p.combo.lineEdit().text().strip():
            p.reload_labs()   # liste des machines à jour (créées ou renommées dans un autre sous-onglet)
            p.combo.setEditText("")
        if p is not None and p.lab is not None and self.win.bottom.indexOf(p.console) >= 0 \
                and getattr(self.win.bottom.currentWidget(), "objectName", lambda: "")().startswith("ssh_term"):
            self.win.bottom.setCurrentWidget(p.console)   # le terminal suit la machine regardée

    def close_session(self, i):
        panel = self.tabs.widget(i)
        if panel is None:
            return
        if self.tabs.count() == 1:          # la dernière : on la déconnecte, sans la retirer
            if HAS_SSH and panel.lab is not None:
                panel.disconnect()
            return
        if HAS_SSH and panel.lab is not None:
            if QMessageBox.question(self, "Fermer la connexion",
                                    f"Se déconnecter de « {self.title(panel)} » ?") != QMessageBox.StandardButton.Yes:
                return
            panel.disconnect()
        panel.shutdown()
        j = self.win.bottom.indexOf(panel.console)
        if j >= 0:
            self.win.bottom.removeTab(j)
        panel.console.deleteLater()
        self.tabs.removeTab(i)
        panel.deleteLater()

    # ---- utilisés par la fenêtre principale
    def panel_for(self, ed):
        """La connexion d'où vient le fichier `ed` (connectée de préférence)."""
        same = [p for p in self.panels() if HAS_SSH and p.lab is not None and p.lab["id"] == ed.remote_lab]
        return (same or [self.current_panel()])[0]

    def upload(self, ed, then=None):
        self.panel_for(ed).upload(ed, then)

    def run_remote(self, ed, debug=False):
        self.panel_for(ed).run_remote(ed, debug)

    def connected(self):
        return any(HAS_SSH and p.lab is not None for p in self.panels())

    def shutdown(self):
        for p in self.panels():
            p.shutdown()
