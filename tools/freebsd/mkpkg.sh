#!/bin/bash
# tools/freebsd/mkpkg.sh VERSION — пакет FreeBSD (`pkg add grazysai-VERSION.pkg`) из
# dist/freebsd/GrazySAI. Пакет собирает сам `pkg create` в виртуальной машине FreeBSD
# (~/.local/share/vms/freebsd: run.sh, ssh.sh): контрольные суммы pkg на Linux не повторить.
# Если машина не запущена — запускается и после сборки выключается. Итог: dist/freebsd/grazysai-VERSION.pkg.
set -e
V=$1; [ -n "$V" ] || { echo "нужна версия"; exit 1; }
cd "$(dirname "$0")/../.."
VM=${FREEBSD_VM:-$HOME/.local/share/vms/freebsd}
SSH="$VM/ssh.sh"
SCP="scp -q -o BatchMode=yes -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -i $HOME/.ssh/fbsdvm_ed25519 -P 2222"
started=
if ! "$SSH" true 2>/dev/null; then
  (cd "$VM" && env -u WAYLAND_DISPLAY -u DISPLAY setsid ./run.sh >qemu.log 2>&1 &)
  started=1
  for i in $(seq 1 90); do "$SSH" true 2>/dev/null && break; sleep 10; done
fi
# версии зависимостей — какие сейчас в репозитории FreeBSD
deps=$("$SSH" 'for p in mesa-libs mesa-dri libX11 libXcursor libXi libXrandr libxkbcommon; do pkg rquery "%n %o %v" $p 2>/dev/null | head -1; done')
dep_ucl=$(echo "$deps" | awk 'NF==3{printf "  %s: {origin: %s, version: \"%s\"}\n", $1, $2, $3}')
cat > dist/freebsd/m.ucl <<UCL
name: grazysai
origin: graphics/grazysai
version: "$V"
comment: "Painting program in the style of PaintTool SAI 2"
maintainer: "grazyping@users.noreply.github.com"
www: "https://github.com/grazyping/GrazySAI"
prefix: /usr/local
licenselogic: single
licenses: [MIT]
categories: [graphics]
desc: <<EOD
GrazySAI is a painting program modelled on PaintTool SAI 2: SAI-like brushes,
.sai2 files, linework layers, 3D layer, brush library. Pen pressure and tilt
come through XInput 2.
EOD
deps: {
$dep_ucl
}
files: {
  /usr/local/bin/GrazySAI: {uname: root, gname: wheel, perm: 0755}
  /usr/local/share/applications/grazysai.desktop: {uname: root, gname: wheel, perm: 0644}
  /usr/local/share/doc/grazysai/MANUAL.md: {uname: root, gname: wheel, perm: 0644}
  /usr/local/share/doc/grazysai/РУКОВОДСТВО.md: {uname: root, gname: wheel, perm: 0644}
  /usr/local/share/doc/grazysai/LICENSE: {uname: root, gname: wheel, perm: 0644}
}
UCL
"$SSH" 'rm -rf pk && mkdir -p pk/st/usr/local/bin pk/st/usr/local/share/applications pk/st/usr/local/share/doc/grazysai pk/out'
$SCP dist/freebsd/GrazySAI gs@127.0.0.1:pk/st/usr/local/bin/GrazySAI
$SCP tools/freebsd/grazysai.desktop gs@127.0.0.1:pk/st/usr/local/share/applications/grazysai.desktop
$SCP docs/manual.en.md gs@127.0.0.1:pk/st/usr/local/share/doc/grazysai/MANUAL.md
$SCP docs/manual.md "gs@127.0.0.1:pk/st/usr/local/share/doc/grazysai/РУКОВОДСТВО.md"
$SCP LICENSE gs@127.0.0.1:pk/st/usr/local/share/doc/grazysai/LICENSE
$SCP dist/freebsd/m.ucl gs@127.0.0.1:pk/m.ucl
"$SSH" 'cd pk && pkg create -M m.ucl -r st -o out'
$SCP "gs@127.0.0.1:pk/out/grazysai-$V.pkg" dist/freebsd/
[ -n "$started" ] && "$SSH" 'echo | su -m root -c "shutdown -p now"' >/dev/null 2>&1 || true
ls -la dist/freebsd/grazysai-$V.pkg
