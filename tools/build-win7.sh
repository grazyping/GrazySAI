#!/bin/bash
# tools/build-win7.sh — сборка GrazySAI для Windows 7 и 8 (x86_64): ночной Rust с
# -Zbuild-std под цель x86_64-win7-windows-gnu, llvm-mingw; затем таблица импорта правится
# под Windows 7 (tools/win7/patch_imports.py) и рядом кладётся gspathcch.dll.
# Нужно: Windows 7 SP1 с обновлением универсальной среды C (KB2999226) и OpenGL 2.0+.
# Итог: dist/win7/GrazySAI.exe и gspathcch.dll.
set -e
cd "$(dirname "$0")/.."
M=${LLVM_MINGW:-$HOME/.local/opt/llvm-mingw}
NIGHTLY=${WIN7_NIGHTLY:-nightly-2026-07-01}
T=x86_64-win7-windows-gnu
STUBS=target/win7/stubs
mkdir -p $STUBS dist/win7
# у llvm-mingw нет libgcc: раскрутка — из libunwind, остальное даёт compiler-rt
cp $M/x86_64-w64-mingw32/lib/libunwind.a $STUBS/libgcc_eh.a
$M/bin/llvm-ar rcs $STUBS/libgcc.a
export PATH=$M/bin:$PATH
export CARGO_TARGET_X86_64_WIN7_WINDOWS_GNU_LINKER=$M/bin/x86_64-w64-mingw32-clang
export CARGO_TARGET_X86_64_WIN7_WINDOWS_GNU_RUSTFLAGS="-C target-feature=+crt-static -L native=$M/x86_64-w64-mingw32/lib -L native=$PWD/$STUBS"
export CC_x86_64_win7_windows_gnu=$M/bin/x86_64-w64-mingw32-clang CXX_x86_64_win7_windows_gnu=$M/bin/x86_64-w64-mingw32-clang++
export AR_x86_64_win7_windows_gnu=$M/bin/llvm-ar CXXSTDLIB_x86_64_win7_windows_gnu=static=c++
cargo +$NIGHTLY build --release -p paint-app --target $T -Zbuild-std=std,panic_abort --target-dir target/win7
cp target/win7/$T/release/GrazySAI.exe dist/win7/GrazySAI.exe
python3 tools/win7/patch_imports.py dist/win7/GrazySAI.exe
$M/bin/x86_64-w64-mingw32-clang -O2 -shared -o dist/win7/gspathcch.dll tools/win7/gspathcch.c
$M/bin/llvm-objdump -p dist/win7/GrazySAI.exe | grep 'DLL Name' | sort -u
