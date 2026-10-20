"""Zip-архив выпуска с именами в UTF-8 (флаг 0x800): Info-ZIP его не ставит, и Windows
показывает русские имена (РУКОВОДСТВО.md) нечитаемыми.  python3 -I tools/mkzip.py OUT.zip DIR"""
import os, sys, zipfile
out, root = sys.argv[1], sys.argv[2].rstrip('/')
base = os.path.dirname(os.path.abspath(root))
with zipfile.ZipFile(out, 'w', zipfile.ZIP_DEFLATED, compresslevel=9) as z:
    for d, dirs, files in os.walk(root):
        dirs.sort()
        for f in sorted(files):
            p = os.path.join(d, f)
            z.write(p, os.path.relpath(p, base))
