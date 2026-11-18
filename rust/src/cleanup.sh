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
