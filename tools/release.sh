#!/bin/bash
# tools/release.sh 0.8.7 — собрать Linux и Windows и упаковать архивы в dist/release
# (справка рядом с программой, ссылки в .md переписаны под плоскую папку).
set -e
V=$1; [ -n "$V" ] || { echo "нужна версия"; exit 1; }
cd "$(dirname "$0")/.."
ROOT=$(pwd)
# адреса из разбора SAI в открытый репозиторий не попадают — правила описываются своими словами
if git ls-files -z | xargs -0 grep -l -E '(FUN|DAT|LAB)_14[0-9a-f]{5,}'; then
  echo "в файлах выше — адреса SAI; убрать перед выпуском"; exit 1
fi
cargo build -q --release -p paint-app -p paint-studio -p paint-convert
cargo build -q --release -p paint-app -p paint-studio -p paint-convert --target x86_64-pc-windows-gnullvm
cd dist/release
for os in linux windows; do
  d=GrazySAI-$V-$os-x86_64; rm -rf $d; mkdir $d
  cp $ROOT/LICENSE $ROOT/README.md $ROOT/README.ru.md $ROOT/ROADMAP.md $ROOT/ROADMAP.en.md $d/
  cp $ROOT/docs/manual.md $d/РУКОВОДСТВО.md; cp $ROOT/docs/manual.en.md $d/MANUAL.md
  # инструкции для авторов кистей и плагинов — рядом с руководством (ссылки в нём на соседние файлы)
  cp $ROOT/docs/35-gsbrush-format.md $ROOT/docs/36-plugins.md $ROOT/docs/36-plugins.en.md $ROOT/docs/37-brushes-guide.md $ROOT/docs/37-brushes-guide.en.md $d/
  sed -i 's#(docs/manual.en.md)#(MANUAL.md)#' $d/README.md
  sed -i 's#(docs/manual.md)#(РУКОВОДСТВО.md)#' $d/README.ru.md
  sed -i 's#(manual.en.md)#(MANUAL.md)#; s#(\.\./ROADMAP.md)#(ROADMAP.md)#' $d/РУКОВОДСТВО.md
  sed -i 's#(manual.md)#(РУКОВОДСТВО.md)#; s#(\.\./ROADMAP.en.md)#(ROADMAP.en.md)#' $d/MANUAL.md
done
cp $ROOT/target/release/GrazySAI $ROOT/target/release/GrazySAI-Studio $ROOT/target/release/GrazySAI-Convert GrazySAI-$V-linux-x86_64/
cp $ROOT/target/x86_64-pc-windows-gnullvm/release/GrazySAI.exe $ROOT/target/x86_64-pc-windows-gnullvm/release/GrazySAI-Studio.exe $ROOT/target/x86_64-pc-windows-gnullvm/release/GrazySAI-Convert.exe GrazySAI-$V-windows-x86_64/
rm -f GrazySAI-$V-*.tar.gz GrazySAI-$V-*.zip
tar czf GrazySAI-$V-linux-x86_64.tar.gz GrazySAI-$V-linux-x86_64
python3 -I "$ROOT/tools/mkzip.py" GrazySAI-$V-windows-x86_64.zip GrazySAI-$V-windows-x86_64
# переход со старого имени: версии до 0.8.42 (SOsai) ищут архив SOsai-…, в нём папку SOsai-…
# и файл SOsai(.exe) — кладём туда новую программу под старым именем
for os in linux windows; do
  o=SOsai-$V-$os-x86_64; rm -rf $o; cp -r GrazySAI-$V-$os-x86_64 $o
done
mv SOsai-$V-linux-x86_64/GrazySAI SOsai-$V-linux-x86_64/SOsai
mv SOsai-$V-windows-x86_64/GrazySAI.exe SOsai-$V-windows-x86_64/SOsai.exe
rm -f SOsai-$V-*.tar.gz SOsai-$V-*.zip
tar czf SOsai-$V-linux-x86_64.tar.gz SOsai-$V-linux-x86_64
python3 -I "$ROOT/tools/mkzip.py" SOsai-$V-windows-x86_64.zip SOsai-$V-windows-x86_64
# FreeBSD и Windows 7/8 — отдельные сборки (tools/build-freebsd.sh, tools/build-win7.sh), если
# есть корень FreeBSD и ночной Rust; только рисовалка, справка — как у основных
if [ -z "$NO_EXTRA" ]; then
  (cd "$ROOT" && tools/build-freebsd.sh >/dev/null && tools/build-win7.sh >/dev/null)
  for os in freebsd windows7; do
    d=GrazySAI-$V-$os-x86_64; rm -rf $d; cp -r GrazySAI-$V-linux-x86_64 $d; rm -f $d/GrazySAI $d/GrazySAI-Studio $d/GrazySAI-Convert
  done
  cp $ROOT/dist/freebsd/GrazySAI GrazySAI-$V-freebsd-x86_64/
  cp $ROOT/dist/win7/GrazySAI.exe $ROOT/dist/win7/gspathcch.dll GrazySAI-$V-windows7-x86_64/
  rm -f GrazySAI-$V-freebsd-*.tar.gz GrazySAI-$V-windows7-*.zip
  tar czf GrazySAI-$V-freebsd-x86_64.tar.gz GrazySAI-$V-freebsd-x86_64
  # пакет FreeBSD — в виртуальной машине FreeBSD, если она есть
  if [ -x "${FREEBSD_VM:-$HOME/.local/share/vms/freebsd}/ssh.sh" ]; then
    (cd "$ROOT" && tools/freebsd/mkpkg.sh "$V" >/dev/null) && cp $ROOT/dist/freebsd/grazysai-$V.pkg .
  fi
  python3 -I "$ROOT/tools/mkzip.py" GrazySAI-$V-windows7-x86_64.zip GrazySAI-$V-windows7-x86_64
fi
ls -la GrazySAI-$V-*.tar.gz GrazySAI-$V-*.zip SOsai-$V-*.tar.gz SOsai-$V-*.zip
