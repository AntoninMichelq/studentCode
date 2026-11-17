"""Thèmes de couleurs de StudentCode.

Le thème choisi est lu au lancement ; en changer relance l'application
(la session est restaurée, donc on retrouve tout à l'identique).
"""
from PyQt6.QtCore import QSettings

#          fond     fond+   survol   texte    comment. marge    bleu     cyan     vert     violet   orange   rouge    jaune    turq.    sélect.  ligne↴   erreur
KEYS = ("BG", "BG_DARK", "BG_HL", "FG", "COMMENT", "GUTTER", "BLUE", "CYAN", "GREEN", "MAGENTA",
        "ORANGE", "RED", "YELLOW", "TEAL", "SEL", "CURLINE_BG", "ERR_BG")
THEMES = {
    "Tokyo Night": ("#1a1b26", "#16161e", "#292e42", "#c0caf5", "#565f89", "#3b4261", "#7aa2f7", "#7dcfff",
                    "#9ece6a", "#bb9af7", "#ff9e64", "#f7768e", "#e0af68", "#73daca", "#283457", "#4d4420", "#5a2233"),
    "Tokyo Night Storm": ("#24283b", "#1f2335", "#2f354f", "#c0caf5", "#636da6", "#3b4261", "#7aa2f7", "#7dcfff",
                          "#9ece6a", "#bb9af7", "#ff9e64", "#f7768e", "#e0af68", "#73daca", "#2e3c64", "#544a22", "#5c2a3b"),
    "Catppuccin Mocha": ("#1e1e2e", "#181825", "#313244", "#cdd6f4", "#6c7086", "#45475a", "#89b4fa", "#89dceb",
                         "#a6e3a1", "#cba6f7", "#fab387", "#f38ba8", "#f9e2af", "#94e2d5", "#3d4466", "#524a2a", "#5a2d3d"),
    "Everforest": ("#2d353b", "#272e33", "#3d484d", "#d3c6aa", "#859289", "#56635f", "#7fbbb3", "#83c092",
                   "#a7c080", "#d699b6", "#e69875", "#e67e80", "#dbbc7f", "#83c092", "#475258", "#55522f", "#5c3f45"),
    "Gruvbox": ("#282828", "#1d2021", "#3c3836", "#ebdbb2", "#928374", "#504945", "#83a598", "#8ec07c",
                "#b8bb26", "#d3869b", "#fe8019", "#fb4934", "#fabd2f", "#8ec07c", "#504945", "#5a4d1c", "#5c2a27"),
    "Nord": ("#2e3440", "#292e39", "#3b4252", "#d8dee9", "#6f7a8f", "#4c566a", "#81a1c1", "#88c0d0",
             "#a3be8c", "#b48ead", "#d08770", "#bf616a", "#ebcb8b", "#8fbcbb", "#434c5e", "#5a5234", "#5b3a40"),
    "Dracula": ("#282a36", "#21222c", "#343746", "#f8f8f2", "#6272a4", "#44475a", "#8be9fd", "#8be9fd",
                "#50fa7b", "#bd93f9", "#ffb86c", "#ff5555", "#f1fa8c", "#50fa7b", "#44475a", "#56552a", "#5e2d33"),
    "Rosé Pine": ("#191724", "#14121d", "#26233a", "#e0def4", "#6e6a86", "#403d52", "#9ccfd8", "#9ccfd8",
                  "#95b1ac", "#c4a7e7", "#ebbcba", "#eb6f92", "#f6c177", "#9ccfd8", "#312f44", "#4f4530", "#532c3f"),
    "Tokyo Night Day (clair)": ("#e1e2e7", "#d5d6db", "#c4c8da", "#3760bf", "#848cb5", "#a8aecb", "#2e7de9", "#007197",
                                "#587539", "#9854f1", "#b15c00", "#f52a65", "#8c6c3e", "#118c74", "#b7c1e3", "#f0e2b0", "#f5c9d3"),
    "Catppuccin Latte (clair)": ("#eff1f5", "#e6e9ef", "#ccd0da", "#4c4f69", "#8c8fa1", "#acb0be", "#1e66f5", "#04a5e5",
                                 "#40a02b", "#8839ef", "#fe640b", "#d20f39", "#df8e1d", "#179299", "#bcc0cc", "#f3e3b3", "#f6c6cf"),
    "Papier (clair)": ("#fbfaf7", "#f1efe9", "#e2dfd6", "#2b2a27", "#8a8679", "#bdb9ac", "#2f6fd0", "#0f7f9b",
                       "#3f7d2c", "#8a4fc7", "#b85c00", "#c8343f", "#9a6f00", "#177e72", "#cfdcf5", "#f5e7b5", "#f6cdd0"),
}
DEFAULT = "Tokyo Night"


def current_name():
    name = QSettings("StudentCode", "StudentCode").value("theme", DEFAULT)
    return name if name in THEMES else DEFAULT


def set_theme(name):
    QSettings("StudentCode", "StudentCode").setValue("theme", name)


NAME = current_name()
P = dict(zip(KEYS, THEMES[NAME]))
LIGHT = "(clair)" in NAME
# correspondance « couleur Tokyo Night -> couleur du thème courant » (pour les icônes)
REMAP = {a.lower(): b for a, b in zip(THEMES[DEFAULT], THEMES[NAME])}
BG, BG_DARK, BG_HL, FG, COMMENT, GUTTER = P["BG"], P["BG_DARK"], P["BG_HL"], P["FG"], P["COMMENT"], P["GUTTER"]
BLUE, CYAN, GREEN, MAGENTA = P["BLUE"], P["CYAN"], P["GREEN"], P["MAGENTA"]
ORANGE, RED, YELLOW, TEAL = P["ORANGE"], P["RED"], P["YELLOW"], P["TEAL"]
SEL, CURLINE_BG, ERR_BG = P["SEL"], P["CURLINE_BG"], P["ERR_BG"]
