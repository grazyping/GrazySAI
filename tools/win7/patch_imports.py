#!/usr/bin/env python3
"""Сборка для Windows 7: имена библиотек в таблице импорта, которых в Windows 7 нет, —
на те, что есть (CoTaskMemFree — в ole32.dll; PathCchStripPrefix — в своей gspathcch.dll рядом).
Использование: patch_imports.py GrazySAI.exe"""
import sys

SWAP = {b"combase.dll": b"ole32.dll", b"api-ms-win-core-path-l1-1-0.dll": b"gspathcch.dll"}
path = sys.argv[1]
data = bytearray(open(path, "rb").read())
for old, new in SWAP.items():
    for variant in (old, old.upper(), old.capitalize()):
        pat = variant + b"\0"
        n = 0
        i = data.find(pat)
        while i >= 0:
            data[i:i + len(pat)] = new + b"\0" * (len(pat) - len(new))
            n += 1
            i = data.find(pat, i + 1)
        if n:
            print(f"{variant.decode()} → {new.decode()}: {n}")
open(path, "wb").write(data)
