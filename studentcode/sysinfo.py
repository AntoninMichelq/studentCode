"""Différences entre systèmes (Linux, Windows) et entre l'app lancée depuis ses sources
et l'app empaquetée (StudentCode.exe)."""
import os
import shutil
import subprocess
import sys

WINDOWS = sys.platform == "win32"
FROZEN = bool(getattr(sys, "frozen", False))
# dossier des fichiers de l'app (scripts d'exécution, icônes) ; dans l'app empaquetée, ils sont
# extraits à côté de l'exécutable
APP_DIR = getattr(sys, "_MEIPASS", None) or os.path.dirname(os.path.abspath(__file__))

_python = None


def python_cmd():
    """Commande qui lance le Python servant à exécuter le code de l'utilisateur, sous forme de
    liste (ex. ["py", "-3"]). Depuis les sources, c'est le Python de l'app ; dans l'app
    empaquetée, c'est celui installé sur l'ordinateur. None s'il n'y en a pas."""
    global _python
    if _python is not None:
        return list(_python) or None
    if not FROZEN:
        _python = [sys.executable]
        return list(_python)
    candidates = []
    if WINDOWS:
        if shutil.which("py"):
            candidates.append(["py", "-3"])
        for name in ("python", "python3"):
            p = shutil.which(name)
            # l'alias « python » du Microsoft Store ouvre le Store au lieu de lancer Python
            if p and "WindowsApps" not in p:
                candidates.append([p])
    else:
        for name in ("python3", "python"):
            p = shutil.which(name)
            if p:
                candidates.append([p])
    for cmd in candidates:
        try:
            r = subprocess.run(cmd + ["-c", "import sys; print(sys.version_info[0])"],
                               capture_output=True, text=True, timeout=8,
                               creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
            if r.returncode == 0 and r.stdout.strip() == "3":
                _python = cmd
                return list(cmd)
        except (OSError, subprocess.SubprocessError):
            continue
    _python = []
    return None


_exe = None


def python_exe():
    """Chemin réel de l'exécutable Python de python_cmd() (pour la complétion), ou None."""
    global _exe
    if _exe is None:
        cmd = python_cmd()
        _exe = ""
        if cmd:
            try:
                r = subprocess.run(cmd + ["-c", "import sys; print(sys.executable)"], capture_output=True,
                                   text=True, timeout=8, creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
                if r.returncode == 0 and os.path.isfile(r.stdout.strip()):
                    _exe = r.stdout.strip()
            except (OSError, subprocess.SubprocessError):
                pass
    return _exe or None


def child_env(env=None):
    """Environnement pour les programmes lancés depuis l'app empaquetée : on retire ce que
    l'empaqueteur a ajouté pour lui-même (bibliothèques internes), qui gênerait les autres."""
    env = dict(os.environ if env is None else env)
    if FROZEN:
        for var in ("LD_LIBRARY_PATH", "QT_PLUGIN_PATH", "QML2_IMPORT_PATH", "QTWEBENGINEPROCESS_PATH",
                    "QTWEBENGINE_RESOURCES_PATH", "QTWEBENGINE_LOCALES_PATH", "PYTHONHOME", "PYTHONPATH",
                    "TCL_LIBRARY", "TK_LIBRARY", "SSL_CERT_FILE", "SSL_CERT_DIR"):
            orig = env.pop(var + "_ORIG", None)
            if orig is not None:
                env[var] = orig
            else:
                env.pop(var, None)
    return env


def prepare_children():
    """Sous Windows, l'app empaquetée restreint la recherche des DLL à son dossier ; les
    programmes lancés en hériteraient (gcc, java, python…) : on remet le comportement normal."""
    if FROZEN and WINDOWS:
        try:
            import ctypes
            ctypes.windll.kernel32.SetDllDirectoryW(None)
        except Exception:
            pass


PYTHON_MISSING = ("Python n'est pas installé sur cet ordinateur : StudentCode en a besoin pour exécuter "
                  "ton code. Installe-le depuis https://www.python.org/downloads/ (coche « Add python.exe "
                  "to PATH »), puis relance StudentCode.")


def restart_cmd():
    """(programme, arguments) pour relancer StudentCode."""
    if FROZEN:
        return sys.executable, []
    return sys.executable, [os.path.join(APP_DIR, "main.py")]


def quote(arg):
    """Met un argument entre guillemets pour la ligne de commande du système."""
    if WINDOWS:
        return subprocess.list2cmdline([arg])
    import shlex
    return shlex.quote(arg)


def shell_cmd(line):
    """(programme, arguments) qui exécutent une ligne de commande du système."""
    if WINDOWS:
        return os.environ.get("COMSPEC", "cmd.exe"), ["/d", "/s", "/c", '"' + line + '"']
    return "bash", ["-c", line]
