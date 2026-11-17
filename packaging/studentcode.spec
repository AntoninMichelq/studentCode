# Recette PyInstaller : `pyinstaller packaging/studentcode.spec` produit dist/StudentCode/
# (un dossier avec l'exécutable StudentCode ou StudentCode.exe et tout ce qu'il lui faut).
import glob
import os
import sys

from PyInstaller.utils.hooks import collect_all, collect_data_files, collect_submodules

ROOT = os.path.abspath(os.path.join(SPECPATH, ".."))
SRC = os.path.join(ROOT, "studentcode")
WINDOWS = sys.platform == "win32"

# fichiers de l'app lus au lancement : scripts confiés au Python de l'ordinateur, icônes
datas = [(f, ".") for f in glob.glob(os.path.join(SRC, "*.svg"))]
datas += [(os.path.join(SRC, f), ".") for f in ("dbg_backend.py", "run_backend.py")]
datas += collect_data_files("jedi") + collect_data_files("parso")
binaries = []
hidden = ["ai", "icons", "langs", "lazy", "pdfview", "remote", "runcmd", "sysinfo", "terminal",
          "theme", "vpn", "pyte", "paramiko", "keyring", "jedi", "parso", "PyQt6.Qsci", "PyQt6.QtSvg",
          "PyQt6.QtWebEngineWidgets", "PyQt6.QtWebEngineCore"]
hidden += collect_submodules("keyring.backends")
for pkg in ("pypdfium2", "pypdfium2_raw") + (("winpty",) if WINDOWS else ()):
    d, b, h = collect_all(pkg)
    datas += d
    binaries += b
    hidden += h

a = Analysis(
    [os.path.join(SRC, "main.py")],
    pathex=[SRC],
    binaries=binaries,
    datas=datas,
    hiddenimports=hidden,
    excludes=["gi", "PIL", "yaml", "tkinter", "PyQt5", "PySide6", "PySide2", "matplotlib", "numpy", "IPython",
              "PyQt6.QtQuick3D", "PyQt6.QtMultimedia", "PyQt6.QtBluetooth", "PyQt6.QtSensors",
              "PyQt6.QtDesigner", "PyQt6.QtCharts", "PyQt6.QtDataVisualization", "PyQt6.Qt3DCore"],
    runtime_hooks=[os.path.join(SPECPATH, "rth_selftest.py")],
    noarchive=False,
)
# Qt est livré entier ; on retire ce que StudentCode n'utilise pas (Qt Quick 3D, contrôles QML,
# multimédia, traductions autres que le français) : l'app passe d'environ 650 à 400 Mo.
DROP = ("quick3d", "quickcontrols2", "quickdialogs2", "quicktemplates2", "quickparticles", "quickeffects",
        "quickvectorimage", "quicktimeline", "quickshapes", "multimedia", "spatialaudio", "shadertools",
        "qt6/qml/", "qt6\\qml\\", "virtualkeyboard", "pdfquick", "labsanimation", "labsqmlmodels",
        "labssettings", "labsfolderlistmodel", "labswavefrontmesh", "labssharedimage", "texttospeech",
        "libqgtk3", "libgtk-3", "libgdk-3")


def keep(entry):
    path = entry[0].replace("\\", "/").lower()
    if any(d.replace("\\", "/") in path for d in DROP):
        return False
    if "/translations/" in path:
        name = path.rsplit("/", 1)[-1]
        return name.startswith(("en-us", "fr")) or name.endswith(("_fr.qm", "_en.qm"))
    return True


a.binaries = [e for e in a.binaries if keep(e)]
a.datas = [e for e in a.datas if keep(e)]
pyz = PYZ(a.pure)
exe = EXE(
    pyz, a.scripts, [],
    exclude_binaries=True,
    name="StudentCode",
    icon=os.path.join(SPECPATH, "icon.ico"),
    console=False,          # pas de fenêtre noire derrière l'app sous Windows
    disable_windowed_traceback=True,   # erreur au démarrage : pas de boîte bloquante (voir rth_selftest)
    upx=False,
    version=os.path.join(SPECPATH, "version_info.txt") if WINDOWS and os.path.isfile(
        os.path.join(SPECPATH, "version_info.txt")) else None,
)
coll = COLLECT(exe, a.binaries, a.datas, name="StudentCode", upx=False)
