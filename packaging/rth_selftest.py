# Exécuté par l'app empaquetée avant StudentCode : pendant l'autotest de la fabrication, une
# erreur au démarrage est écrite dans le fichier de résultat (l'exécutable Windows n'a pas de
# console où l'afficher).
import os
import sys

_out = os.environ.get("STUDENTCODE_SELFTEST_OUT")
if _out:
    def _hook(kind, value, tb):
        import traceback
        with open(_out, "a", encoding="utf-8") as fh:
            fh.write("ERREUR plantage au démarrage :\n" + "".join(traceback.format_exception(kind, value, tb)))
    sys.excepthook = _hook
