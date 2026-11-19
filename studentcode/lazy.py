"""Chargement à la demande des modules lourds : ils ne coûtent de la mémoire qu'une fois utilisés."""
import importlib
import importlib.util


def available(name):
    try:
        return importlib.util.find_spec(name) is not None
    except (ImportError, ValueError):
        return False


class LazyModule:
    """Se comporte comme le module `name`, importé au premier accès à l'un de ses attributs."""

    def __init__(self, name):
        self._name = name
        self._mod = None

    def _load(self):
        if self._mod is None:
            self._mod = importlib.import_module(self._name)
        return self._mod

    def __getattr__(self, attr):
        return getattr(self._load(), attr)

    def __bool__(self):
        return True


def discard(page):
    """Libère la mémoire d'une page web qu'on ne regarde pas (elle sera rechargée au retour)."""
    from PyQt6.QtWebEngineCore import QWebEnginePage
    S = QWebEnginePage.LifecycleState
    try:
        if page.lifecycleState() == S.Active:
            page.setLifecycleState(S.Frozen)
        if page.lifecycleState() == S.Frozen:
            page.setLifecycleState(S.Discarded)
    except Exception:
        pass


def wake(page):
    from PyQt6.QtWebEngineCore import QWebEnginePage
    try:
        if page.lifecycleState() != QWebEnginePage.LifecycleState.Active:
            page.setLifecycleState(QWebEnginePage.LifecycleState.Active)
    except Exception:
        pass
