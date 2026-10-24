#!/bin/bash
# tools/build-freebsd.sh — сборка GrazySAI для FreeBSD (x86_64) на Linux: цель
# x86_64-unknown-freebsd, clang и lld из llvm-mingw, корень системы — base.txz FreeBSD 15.1
# (FREEBSD_SYSROOT, по умолчанию ~/.local/share/vms/freebsd/sysroot). Итог: dist/freebsd/GrazySAI.
set -e
cd "$(dirname "$0")/.."
T=x86_64-unknown-freebsd
export CARGO_TARGET_X86_64_UNKNOWN_FREEBSD_LINKER=$PWD/tools/freebsd/cc
export CC_x86_64_unknown_freebsd=$PWD/tools/freebsd/cc CXX_x86_64_unknown_freebsd=$PWD/tools/freebsd/c++
export AR_x86_64_unknown_freebsd=${LLVM_MINGW:-$HOME/.local/opt/llvm-mingw}/bin/llvm-ar
export CXXSTDLIB_x86_64_unknown_freebsd=c++
cargo build --release -p paint-app --target $T
mkdir -p dist/freebsd
cp target/$T/release/GrazySAI dist/freebsd/GrazySAI
file dist/freebsd/GrazySAI 2>/dev/null || true
