#!/usr/bin/env python3
"""Lanceur de SCode : exécute le script, puis laisse un interpréteur interactif
ouvert dans le même espace de noms (comme PyScripter ou « python -i »).

Usage :  python run_backend.py <script> [args...]
"""
import code
import os
import sys
import traceback
import types

END = "\x1e__LAZYPY_END__%d\x1e"  # signale à l'IDE la fin du programme


def interact(g, rc):
    sys.stdout.flush()
    sys.stderr.flush()
    sys.stdout.write(END % rc)
    sys.stdout.flush()
    sys.ps1, sys.ps2 = ">>> ", "... "
    try:
        code.interact(banner="", local=g, exitmsg="")
    except SystemExit:
        pass


def main():
    script = os.path.abspath(sys.argv[1])
    sys.argv = [script] + sys.argv[2:]
    sys.path[0] = os.path.dirname(script)
    mod = types.ModuleType("__main__")
    mod.__file__ = script
    mod.__builtins__ = __builtins__
    sys.modules["__main__"] = mod
    g = mod.__dict__
    rc = 0
    try:
        with open(script, "rb") as fh:
            src = fh.read()
        exec(compile(src, script, "exec"), g)
    except SystemExit as e:
        rc = e.code if isinstance(e.code, int) else (0 if e.code is None else 1)
    except BaseException as e:
        tb = sys.exc_info()[2]
        traceback.print_exception(type(e), e, tb.tb_next if tb else None)
        rc = 1
    interact(g, rc)
    os._exit(0)


if __name__ == "__main__":
    main()
