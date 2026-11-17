#!/usr/bin/env python3
"""Backend de débogage de LazyPy.

Lancé par l'IDE :  python dbg_backend.py <port> <stop_entry 0|1> <script> [args...]
Dialogue avec l'IDE en JSON (une ligne par message) sur un socket local.
"""
import bdb
import json
import os
import queue
import socket
import sys
import sysconfig
import threading
import traceback
import types

HERE = os.path.abspath(__file__)
MAX_ITEMS = 300


def describe(name, value, expr):
    try:
        r = repr(value)
    except Exception as e:  # repr cassé côté utilisateur
        r = "<repr impossible : %s>" % e
    if len(r) > 300:
        r = r[:300] + "…"
    tname = type(value).__name__
    exp = False
    try:
        if hasattr(value, "shape") and not isinstance(value, type):
            tname += " " + str(tuple(value.shape))
        elif isinstance(value, (dict, list, tuple, set, frozenset, str, bytes)):
            tname += " [%d]" % len(value)
        if isinstance(value, (dict, list, tuple, set, frozenset)):
            exp = len(value) > 0
        elif not callable(value) and not isinstance(value, types.ModuleType):
            d = getattr(value, "__dict__", None)
            exp = isinstance(d, dict) and len(d) > 0
    except Exception:
        pass
    return {"name": name, "type": tname, "repr": r, "expr": expr, "exp": exp}


class Debugger(bdb.Bdb):
    def __init__(self, sock, stop_entry):
        super().__init__()
        self.sock = sock
        self.rfile = sock.makefile("r", encoding="utf-8")
        self.q = queue.Queue()
        self.lock = threading.Lock()
        self.stop_entry = stop_entry
        self.first = True
        self.frames = []
        paths = sysconfig.get_paths()
        libs = {paths.get(k) for k in ("stdlib", "platstdlib", "purelib", "platlib")}
        self.libdirs = tuple(os.path.abspath(p) + os.sep for p in libs if p)

    # ---------------------------------------------------------------- comm
    def send(self, obj):
        data = (json.dumps(obj) + "\n").encode("utf-8")
        with self.lock:
            try:
                self.sock.sendall(data)
            except OSError:
                pass

    def reader(self):
        """Thread : lit les commandes de l'IDE."""
        try:
            for line in self.rfile:
                try:
                    m = json.loads(line)
                except ValueError:
                    continue
                c = m.get("cmd")
                if c == "break":
                    if m["on"]:
                        self.set_break(m["file"], m["line"])
                    else:
                        self.clear_break(m["file"], m["line"])
                elif c == "pause":
                    self.set_step()
                else:
                    self.q.put(m)
        except OSError:
            pass
        os._exit(1)  # l'IDE a fermé la connexion

    # ---------------------------------------------------------------- bdb
    def is_user(self, filename):
        if not filename or filename.startswith("<"):
            return False
        fn = self.canonic(filename)
        if fn == HERE or fn.startswith(self.libdirs):
            return False
        return "site-packages" not in fn and "dist-packages" not in fn

    def stop_here(self, frame):
        # « Just my code » : on ne s'arrête jamais dans les bibliothèques
        if not self.is_user(frame.f_code.co_filename):
            return False
        return super().stop_here(frame)

    def set_continue(self):
        # On garde le traçage actif pour pouvoir ajouter des points d'arrêt
        # et mettre en pause pendant l'exécution.
        self._set_stopinfo(self.botframe, None, -1)

    def user_line(self, frame):
        if self.first:
            self.first = False
            if not self.stop_entry and not self.break_here(frame):
                self.set_continue()
                return
        self.interact(frame)

    def interact(self, frame):
        stack, _ = self.get_stack(frame, None)
        pairs = [(f, ln) for f, ln in stack if self.is_user(f.f_code.co_filename)]
        pairs = pairs[-60:]
        pairs.reverse()  # sommet de pile en premier
        self.frames = [f for f, _ in pairs]
        self.send({"event": "stopped", "stack": [
            {"file": self.canonic(f.f_code.co_filename), "line": ln,
             "func": f.f_code.co_name} for f, ln in pairs]})
        while True:
            m = self.q.get()
            c = m.get("cmd")
            if c == "continue":
                self.set_continue()
                return
            if c == "step":
                self.set_step()
                return
            if c == "next":
                self.set_next(frame)
                return
            if c == "return":
                self.set_return(frame)
                return
            if c == "quit":
                self.set_quit()
                return
            try:
                if c == "vars":
                    self.cmd_vars(m)
                elif c == "children":
                    self.cmd_children(m)
                elif c == "eval":
                    self.cmd_eval(m)
            except Exception:
                self.send({"event": "eval", "text": traceback.format_exc(limit=1),
                           "error": True})

    # ------------------------------------------------------------ commandes
    def frame_at(self, i):
        return self.frames[max(0, min(i, len(self.frames) - 1))]

    def cmd_vars(self, m):
        f = self.frame_at(m.get("frame", 0))

        def listing(d):
            out = []
            for k in list(d)[:MAX_ITEMS * 2]:
                if k.startswith("__") and k.endswith("__"):
                    continue
                out.append(describe(k, d[k], k))
            return out

        if f.f_locals is f.f_globals:
            loc, glo = [], listing(f.f_globals)
        else:
            loc, glo = listing(f.f_locals), listing(f.f_globals)
        self.send({"event": "vars", "frame": m.get("frame", 0),
                   "locals": loc, "globals": glo})

    def cmd_children(self, m):
        f = self.frame_at(m.get("frame", 0))
        expr = m["expr"]
        v = eval(expr, f.f_globals, f.f_locals)
        out = []
        if isinstance(v, dict):
            for i, (k, val) in enumerate(list(v.items())[:MAX_ITEMS]):
                out.append(describe(repr(k), val, "list((%s).values())[%d]" % (expr, i)))
        elif isinstance(v, (list, tuple)):
            for i, val in enumerate(v[:MAX_ITEMS]):
                out.append(describe("[%d]" % i, val, "(%s)[%d]" % (expr, i)))
        elif isinstance(v, (set, frozenset)):
            for i, val in enumerate(list(v)[:MAX_ITEMS]):
                out.append(describe("{%d}" % i, val, "list(%s)[%d]" % (expr, i)))
        else:
            for k, val in list(vars(v).items())[:MAX_ITEMS]:
                out.append(describe(k, val, "(%s).%s" % (expr, k)))
        self.send({"event": "children", "id": m["id"], "items": out})

    def cmd_eval(self, m):
        f = self.frame_at(m.get("frame", 0))
        src = m["expr"]
        error = False
        try:
            try:
                code = compile(src, "<console>", "eval")
            except SyntaxError:
                exec(compile(src, "<console>", "exec"), f.f_globals, f.f_locals)
                text = ""
            else:
                r = eval(code, f.f_globals, f.f_locals)
                text = "" if r is None else repr(r)
        except Exception:
            text = traceback.format_exc(limit=0).strip()
            error = True
        sys.stdout.flush()
        self.send({"event": "eval", "text": text, "error": error})


def main():
    port = int(sys.argv[1])
    stop_entry = sys.argv[2] == "1"
    script = os.path.abspath(sys.argv[3])
    sock = socket.create_connection(("127.0.0.1", port))
    dbg = Debugger(sock, stop_entry)
    init = json.loads(dbg.rfile.readline())
    for f, line in init.get("breaks", []):
        dbg.set_break(f, line)
    threading.Thread(target=dbg.reader, daemon=True).start()

    sys.argv = [script] + sys.argv[4:]
    sys.path[0] = os.path.dirname(script)
    mod = types.ModuleType("__main__")
    mod.__file__ = script
    mod.__builtins__ = __builtins__
    sys.modules["__main__"] = mod
    g = mod.__dict__
    code = 0
    try:
        with open(script, "rb") as fh:
            src = fh.read()
        dbg.run(compile(src, script, "exec"), g, g)
    except SystemExit as e:
        code = e.code if isinstance(e.code, int) else (0 if e.code is None else 1)
    except BaseException as e:
        etype, _, tb = sys.exc_info()
        while tb is not None and not dbg.is_user(tb.tb_frame.f_code.co_filename):
            tb = tb.tb_next
        file = line = None
        t = tb
        while t is not None:
            if dbg.is_user(t.tb_frame.f_code.co_filename):
                file, line = dbg.canonic(t.tb_frame.f_code.co_filename), t.tb_lineno
            t = t.tb_next
        if isinstance(e, SyntaxError) and e.filename:
            file, line = os.path.abspath(e.filename), e.lineno or 1
        traceback.print_exception(etype, e, tb)
        dbg.send({"event": "exception", "file": file, "line": line,
                  "text": "%s: %s" % (etype.__name__, e)})
        code = 1
    # le programme est fini : on laisse un interpréteur ouvert sur ses variables
    sys.stdout.flush()
    sys.stderr.flush()
    sys.stdout.write("\x1e__LAZYPY_END__%d\x1e" % code)
    sys.stdout.flush()
    sys.ps1, sys.ps2 = ">>> ", "... "
    import code as _code
    try:
        _code.interact(banner="", local=g, exitmsg="")
    except SystemExit:
        pass
    os._exit(0)


if __name__ == "__main__":
    main()
