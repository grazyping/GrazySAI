#!/bin/bash
# tools/release.sh 0.8.7 — собрать Linux и Windows и упаковать архивы в dist/release
# (справка рядом с программой, ссылки в .md переписаны под плоскую папку).
set -e
V=$1; [ -n "$V" ] || { echo "нужна версия"; exit 1; }
cd "$(dirname "$0")/.."
# адреса из разбора SAI в открытый репозиторий не попадают — правила описываются своими словами
if git ls-files -z | xargs -0 grep -l -E '(FUN|DAT|LAB)_14[0-9a-f]{5,}'; then
  echo "в файлах выше — адреса SAI; убрать перед выпуском"; exit 1
fi
cargo build -q --release -p paint-app
cargo build -q --release -p paint-app --target x86_64-pc-windows-gnullvm
cd dist/release
for os in linux windows; do
  d=SOsai-$V-$os-x86_64; rm -rf $d; mkdir $d
  cp ../../LICENSE ../../README.md ../../README.ru.md ../../ROADMAP.md ../../ROADMAP.en.md $d/
  cp ../../docs/manual.md $d/РУКОВОДСТВО.md; cp ../../docs/manual.en.md $d/MANUAL.md
  sed -i 's#(docs/manual.en.md)#(MANUAL.md)#' $d/README.md
  sed -i 's#(docs/manual.md)#(РУКОВОДСТВО.md)#' $d/README.ru.md
  sed -i 's#(manual.en.md)#(MANUAL.md)#; s#(\.\./ROADMAP.md)#(ROADMAP.md)#' $d/РУКОВОДСТВО.md
  sed -i 's#(manual.md)#(РУКОВОДСТВО.md)#; s#(\.\./ROADMAP.en.md)#(ROADMAP.en.md)#' $d/MANUAL.md
done
cp ../../target/release/SOsai SOsai-$V-linux-x86_64/
cp ../../target/x86_64-pc-windows-gnullvm/release/SOsai.exe SOsai-$V-windows-x86_64/
rm -f SOsai-$V-*.tar.gz SOsai-$V-*.zip
tar czf SOsai-$V-linux-x86_64.tar.gz SOsai-$V-linux-x86_64
zip -qr SOsai-$V-windows-x86_64.zip SOsai-$V-windows-x86_64
ls -la SOsai-$V-*.tar.gz SOsai-$V-*.zip
