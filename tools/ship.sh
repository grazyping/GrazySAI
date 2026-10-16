#!/bin/bash
# tools/ship.sh NOTES.md VERSION "заголовок коммита" [номера задач…] — весь выпуск одной цепочкой
# (set -e: любой сбой останавливает всё). NOTES.md — заметки выпуска, первая строка **GrazySAI VERSION**.
# Проверки, сборка (tools/release.sh), коммит, метка, отправка, выпуск на GitHub, закрытие задач.
set -euo pipefail
NOTES=$(realpath "$1"); V=$2; TITLE=$3; shift 3
T=$(mktemp -d)
cd "$(dirname "$0")/.."
grep -q "^\*\*GrazySAI $V\*\*" "$NOTES" || { echo "notes.md не для $V"; exit 1; }
old=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
sed -i "s/^version = \"$old\"/version = \"$V\"/" Cargo.toml
sed -i "s/(текущая версия $old)/(текущая версия $V)/" ROADMAP.md
sed -i "s/(current version $old)/(current version $V)/" ROADMAP.en.md
sed -i -E "s/Version [0-9]+\.[0-9]+\.[0-9]+\./Version $V./" README.md
sed -i -E "s/Версия [0-9]+\.[0-9]+\.[0-9]+\./Версия $V./" README.ru.md
grep -q "^version = \"$V\"" Cargo.toml
export GIT_SSH_COMMAND="ssh -i ~/.ssh/github_ed25519 -o IdentitiesOnly=yes"
R=grazyping/GrazySAI
gh pr list -R $R
cargo test -q --workspace >"$T/t.log" 2>&1 || { grep -E 'FAILED|panicked' -A8 "$T/t.log" | head -40; exit 1; }
cargo clippy -q --workspace --all-targets -- -D warnings
tools/release.sh "$V" >"$T/rel.log" 2>&1 || { tail -20 "$T/rel.log"; exit 1; }
git add -A
git commit -q -m "GrazySAI $V: $TITLE

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
git tag "v$V"
git push -q origin HEAD "v$V"
gh release create "v$V" -R $R -t "GrazySAI $V" -F "$NOTES" dist/release/GrazySAI-$V-windows-x86_64.zip dist/release/GrazySAI-$V-linux-x86_64.tar.gz dist/release/SOsai-$V-windows-x86_64.zip dist/release/SOsai-$V-linux-x86_64.tar.gz
for i in "$@"; do gh issue close "$i" -R $R -c "Сделано в $V / Done in $V"; done
echo "выпущено $V"
