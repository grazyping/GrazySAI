#!/bin/bash
# Проверка редактора виртуальным пером без планшета: скрытый экран Xvfb, встроенный сценарий
# `demo` (crates/paint-app/src/virtual_pen.rs), сохранение холста и проверка результата.
#   tools/virtual-pen-check.sh [путь к paint-app]
# Нужны Xvfb (~/.local/opt/xtools или системный) и python3 с PIL.
set -e
APP=${1:-$(dirname "$0")/../target/release/SOsai}
OUT=$(mktemp --suffix=.png)
export PATH=$HOME/.local/opt/xtools/root/usr/bin:$PATH LD_LIBRARY_PATH=$HOME/.local/opt/xtools/root/usr/lib/x86_64-linux-gnu
unset WAYLAND_DISPLAY
Xvfb :78 -screen 0 1600x1000x24 -nolisten tcp >/dev/null 2>&1 &
XV=$!
trap 'kill $XV 2>/dev/null; rm -f "$OUT"' EXIT
sleep 2
DISPLAY=:78 LIBGL_ALWAYS_SOFTWARE=1 PAINT_VIRTUAL_PEN=demo PAINT_VIRTUAL_PEN_OUT="$OUT" timeout 90 "$APP" 2>&1 | tail -1
python3 -I - "$OUT" <<'PY'
import sys
from PIL import Image
im = Image.open(sys.argv[1]).convert('L'); px = im.load()
def ink(x, y0, y1):            # самая тёмная точка столбца в полосе
    return 255 - min(px[x, y] for y in range(y0, y1))
def width(x, y0, y1, level=40):
    return sum(1 for y in range(y0, y1) if 255 - px[x, y] > level)
light_mid, light_end = ink(320, 205, 235), max(ink(x, 205, 235) for x in range(570, 590))
strong = width(320, 255, 285)
light = width(320, 205, 235, level=10)
erased = ink(320, 60, 200)
checks = [
    ("лёгкий нажим тоньше сильного", light < strong, f"{light} < {strong}"),
    ("нет кляксы на конце лёгкой линии", light_end <= light_mid + 20, f"конец {light_end}, середина {light_mid}"),
    ("ластик стёр волну", erased < 30, f"остаток {erased}"),
]
bad = 0
for name, ok, info in checks:
    print(("ok   " if ok else "FAIL ") + name + " — " + info)
    bad += not ok
sys.exit(bad)
PY
