#!/bin/bash
# tools/wm-check.sh WM OUT_DIR — GrazySAI под оконным менеджером WM на скрытом экране (Xvfb :85):
# окно программы, меню «Файл», плавающий вид (отдельное окно системы). Менеджеры без установки —
# распакованные пакеты Debian в ~/.local/opt/wms/root (apt-get download + dpkg -x).
WM=$1; OUT=${2:-.}; mkdir -p "$OUT"
R=$HOME/.local/opt/wms/root
export PATH=$R/usr/bin:$HOME/.local/opt/xtools/root/usr/bin:$PATH
export LD_LIBRARY_PATH=$R/usr/lib/x86_64-linux-gnu:$HOME/.local/opt/xtools/root/usr/lib/x86_64-linux-gnu
unset WAYLAND_DISPLAY
# данные и схемы настроек распакованных пакетов (marco, metacity); схемы собрать один раз:
# glib-compile-schemas ~/.local/opt/wms/root/usr/share/glib-2.0/schemas
export XDG_DATA_DIRS=$R/usr/share:/usr/share GSETTINGS_SCHEMA_DIR=$R/usr/share/glib-2.0/schemas
EXE=${GS_EXE:-$(cd "$(dirname "$0")/.." && pwd)/target/release/GrazySAI}
Xvfb :85 -screen 0 1400x940x24 -nolisten tcp >/dev/null 2>&1 & XV=$!; sleep 1.5
export DISPLAY=:85 HOME_REAL=$HOME
TMPH=$(mktemp -d)   # менеджеры пишут свои настройки — во временный дом
case $WM in
  awesome) HOME=$TMPH awesome -c $R/etc/xdg/awesome/rc.lua >/dev/null 2>&1 & ;;
  i3) printf 'set $mod Mod4\nfont pango:monospace 8\n' > $TMPH/i3.conf; HOME=$TMPH i3 -c $TMPH/i3.conf >/dev/null 2>&1 & ;;
  *) HOME=$TMPH $WM >/dev/null 2>&1 & ;;
esac
WP=$!; sleep 2
( LIBGL_ALWAYS_SOFTWARE=1 PAINT_SETTINGS=off PAINT_NATIVE_DIALOGS=off PAINT_LANG=${L:-en} timeout 60 "$EXE" >"$OUT/ui.log" 2>&1 ) & AP=$!
sleep ${GS_WAIT:-6}
grab(){ python3 -I -c "from PIL import ImageGrab; ImageGrab.grab(xdisplay=':85').save('$OUT/$1.png')"; }
W=$(xdotool search --name GrazySAI | head -1); xdotool windowactivate --sync $W 2>/dev/null
grab "$WM-1-window"
xdotool key ctrl+o 2>/dev/null; sleep 0.2; xdotool key Escape
G=$(xdotool getwindowgeometry $W 2>/dev/null | tr '\n' ' ')
echo "$WM: окно $W $G" > "$OUT/$WM.txt"
xdotool search --name GrazySAI 2>/dev/null | wc -l >> "$OUT/$WM.txt"
kill $AP $WP $XV 2>/dev/null; wait 2>/dev/null; rm -rf "$TMPH"
