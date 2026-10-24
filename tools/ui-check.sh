#!/bin/bash
# tools/ui-check.sh STEPS.sh [OUT_DIR] — запустить GrazySAI на скрытом экране (Xvfb :83, 1400×940)
# и выполнить шаги из файла (bash): click X Y, stroke X0 Y0 X1 Y1, grab ИМЯ (снимок в OUT_DIR/ИМЯ.png),
# любые команды xdotool. Переменные: L — язык (en), GS_HOME — подменить домашнюю папку,
# GS_SETTINGS=<путь к settings.txt> — с настройками (по умолчанию выключены); GS_TIMEOUT, GS_WAIT — сколько жить и ждать запуска (с); GS_EXE — другой исполняемый файл; GS_WINE=1 — запустить Windows-сборку (.exe) под Wine (GS_WINEPREFIX — другой префикс)
# Системные окна выбора файла выключены (PAINT_NATIVE_DIALOGS=off): иначе портал открывает их на настоящем рабочем столе.
# (свой префикс ~/.local/share/wineprefixes/grazysai, вывод Wine — в ui.log). Настройки выключены (PAINT_SETTINGS=off).
export PATH=$HOME/.local/opt/xtools/root/usr/bin:$PATH LD_LIBRARY_PATH=$HOME/.local/opt/xtools/root/usr/lib/x86_64-linux-gnu
unset WAYLAND_DISPLAY
STEPS=$1
OUT=${2:-$(dirname "$STEPS")}
EXE=${GS_EXE:-$(cd "$(dirname "$0")/.." && pwd)/target/release/GrazySAI}
Xvfb :83 -screen 0 1400x940x24 -nolisten tcp >/dev/null 2>&1 &
XV=$!; sleep 1.5
( [ -n "$GS_HOME" ] && export HOME=$GS_HOME; DISPLAY=:83 LIBGL_ALWAYS_SOFTWARE=1 PAINT_NATIVE_DIALOGS=off PAINT_SETTINGS=${GS_SETTINGS:-off} PAINT_LANG=${L:-en} timeout ${GS_TIMEOUT:-120} ${GS_WINE:+env WINEPREFIX=${GS_WINEPREFIX:-$HOME/.local/share/wineprefixes/grazysai} WINEDLLOVERRIDES=mscoree,mshtml= wine} "$EXE" >"$OUT/ui.log" 2>&1 ) &
AP=$!; sleep ${GS_WAIT:-5}
export DISPLAY=:83
W=$(xdotool search --name GrazySAI | head -1); xdotool windowactivate --sync $W 2>/dev/null; xdotool windowfocus $W
grab(){ python3 -I -c "from PIL import ImageGrab; ImageGrab.grab(xdisplay=':83').save('$OUT/$1.png')"; }
click(){ xdotool mousemove $1 $2; sleep 0.3; xdotool click 1; sleep 0.8; }
stroke(){ xdotool mousemove $1 $2; xdotool mousedown 1; for i in $(seq 0 30); do xdotool mousemove $(( $1 + ($3-$1)*i/30 )) $(( $2 + ($4-$2)*i/30 )); sleep 0.02; done; xdotool mouseup 1; sleep 0.4; }
[ -n "$STEPS" ] && . "$STEPS"
kill $AP $XV 2>/dev/null; wait 2>/dev/null
