"""Commandes qui exécutent un fichier selon son type : C et C++ sont compilés puis lancés,
Java compile tous les .java du dossier puis lance la classe du fichier, les scripts sont
confiés à leur interpréteur. Python passe ailleurs (exécution avec débogueur possible)."""
import glob
import hashlib
import os
import re
import shlex
import shutil
import subprocess
import tempfile

# Les printf d'un programme C n'apparaissent qu'à la fin quand la sortie est redirigée :
# ce petit fichier, compilé avec le programme, coupe ce tampon dès le démarrage.
NOBUF_C = ("#include <stdio.h>\n"
           "__attribute__((constructor)) static void studentcode_nobuf(void) {\n"
           "    setvbuf(stdout, NULL, _IONBF, 0);\n    setvbuf(stderr, NULL, _IONBF, 0);\n}\n")


def posix_command(path, python="python3 -u", unbuffer=True):
    """Commande bash (Linux, macOS, et machines distantes en SSH)."""
    q = shlex.quote
    folder, name = path.rsplit("/", 1) if "/" in path else (".", path)
    stem, ext = (name.rsplit(".", 1) + [""])[:2]
    ext = "." + ext.lower()
    out = "/tmp/studentcode-build/" + hashlib.md5(folder.encode()).hexdigest()[:10]
    nobuf = "stdbuf -o0 -e0 " if unbuffer else ""   # affichage immédiat des printf
    simple = {".py": python, ".sh": "bash", ".bash": "bash", ".pl": "perl", ".rb": "ruby", ".js": "node"}
    if ext in simple:
        return f"cd {q(folder)} && exec {simple[ext]} {q(name)}"
    if ext in (".c", ".cpp", ".cc", ".cxx"):
        cc = "gcc -Wall -g" if ext == ".c" else "g++ -Wall -g"
        return (f"cd {q(folder)} && mkdir -p {out} && {cc} {q(name)} -o {out}/{q(stem)} -lm "
                f"&& exec {nobuf}{out}/{q(stem)}")
    if ext == ".java":
        # la classe à lancer : nom du fichier, précédé du paquet s'il y en a un
        return (f"cd {q(folder)} && mkdir -p {out} && javac -d {out} *.java "
                f"&& pkg=$(sed -n 's/^[[:space:]]*package[[:space:]]\\+\\([A-Za-z0-9_.]*\\)[[:space:]]*;.*/\\1./p' {q(name)} | head -1) "
                f"&& exec java -cp {out} \"${{pkg}}{stem}\"")
    return None


def java_package(path):
    try:
        with open(path, encoding="utf-8", errors="replace") as fh:
            m = re.search(r"^\s*package\s+([\w.]+)\s*;", fh.read(), re.M)
        return m.group(1) + "." if m else ""
    except OSError:
        return ""


def windows_command(path):
    """(ligne pour cmd.exe, None) ou (None, explication) sous Windows."""
    q = lambda a: subprocess.list2cmdline([a])
    folder, name = os.path.split(os.path.abspath(path))
    stem, ext = os.path.splitext(name)
    ext = ext.lower()
    build = os.path.join(tempfile.gettempdir(), "studentcode-build", hashlib.md5(folder.encode()).hexdigest()[:10])
    cd = f"cd /d {q(folder)}"
    if ext in (".c", ".cpp", ".cc", ".cxx"):
        cc = "gcc" if ext == ".c" else "g++"
        if not shutil.which(cc):
            return None, (f"Le compilateur {cc} est introuvable. Installe MinGW-w64 (par exemple avec "
                          "MSYS2 : https://www.msys2.org) et ajoute son dossier bin au PATH.")
        os.makedirs(build, exist_ok=True)
        helper = os.path.join(build, "studentcode_nobuf.c")
        with open(helper, "w") as fh:
            fh.write(NOBUF_C)
        exe = os.path.join(build, stem + ".exe")
        return f"{cd} && {cc} -Wall -g {q(name)} {q(helper)} -o {q(exe)} && {q(exe)}", None
    if ext == ".java":
        if not (shutil.which("javac") and shutil.which("java")):
            return None, ("Java (JDK) est introuvable. Installe un JDK, par exemple Temurin : "
                          "https://adoptium.net, en cochant l'ajout au PATH.")
        os.makedirs(build, exist_ok=True)
        sources = " ".join(q(os.path.basename(f)) for f in sorted(glob.glob(os.path.join(folder, "*.java"))))
        return f"{cd} && javac -d {q(build)} {sources} && java -cp {q(build)} {java_package(path)}{stem}", None
    if ext in (".bat", ".cmd"):
        return f"{cd} && call {q(name)}", None
    if ext == ".ps1":
        return f"{cd} && powershell -NoProfile -ExecutionPolicy Bypass -File {q(name)}", None
    tools = {".js": "node", ".pl": "perl", ".rb": "ruby", ".sh": "bash", ".bash": "bash"}
    if ext in tools:
        tool = tools[ext]
        if not shutil.which(tool):
            extra = " (installe Git for Windows pour avoir bash)" if tool == "bash" else ""
            return None, f"« {tool} » est introuvable sur cet ordinateur{extra}."
        return f"{cd} && {tool} {q(name)}", None
    return None, "Je ne sais pas exécuter ce type de fichier."
