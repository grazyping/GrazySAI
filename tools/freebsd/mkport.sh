#!/bin/bash
# tools/freebsd/mkport.sh VERSION — файлы порта graphics/grazysai для дерева портов FreeBSD в
# dist/freebsd-port/grazysai: Makefile (CARGO_CRATES из Cargo.lock), distinfo, pkg-descr, pkg-plist.
# Контрольные суммы крейтов — из ~/.cargo/registry/cache, архива выпуска — с GitHub (метка vVERSION
# должна уже быть). Сборку порта и отправку в FreeBSD Bugzilla этот скрипт не делает.
set -euo pipefail
V=${1:?нужна версия}
cd "$(dirname "$0")/../.."
OUT=dist/freebsd-port/grazysai
mkdir -p "$OUT"
cargo fetch -q
export V OUT
python3 -I - <<'PY'
import glob, hashlib, os, re, subprocess, urllib.request
v, out = os.environ['V'], os.environ['OUT']
lock = open('Cargo.lock').read()
crates = []
for block in lock.split('[[package]]')[1:]:
    if 'source = "registry+' not in block:
        continue
    crates.append((re.search(r'name = "(.*)"', block)[1], re.search(r'version = "(.*)"', block)[1]))
crates.sort()
dist = []
tar = f'grazyping-GrazySAI-v{v}_GH0.tar.gz'
data = urllib.request.urlopen(f'https://codeload.github.com/grazyping/GrazySAI/tar.gz/refs/tags/v{v}').read()
dist.append((tar, hashlib.sha256(data).hexdigest(), len(data)))
cache = glob.glob(os.path.expanduser('~/.cargo/registry/cache/*/'))
for n, ver in crates:
    f = next(p for p in (os.path.join(c, f'{n}-{ver}.crate') for c in cache) if os.path.exists(p))
    b = open(f, 'rb').read()
    dist.append((f'rust/crates/{n}-{ver}.crate', hashlib.sha256(b).hexdigest(), len(b)))
with open(f'{out}/distinfo', 'w') as o:
    for name, sha, size in dist:
        o.write(f'SHA256 ({name}) = {sha}\nSIZE ({name}) = {size}\n')
lines = ' \\\n\t\t'.join(f'{n}-{ver}' for n, ver in crates)
open(f'{out}/Makefile', 'w').write(f'''PORTNAME=	grazysai
DISTVERSIONPREFIX=	v
DISTVERSION=	{v}
CATEGORIES=	graphics

MAINTAINER=	ports@FreeBSD.org
COMMENT=	Painting program modelled on PaintTool SAI 2
WWW=		https://github.com/grazyping/GrazySAI

LICENSE=	MIT
LICENSE_FILE=	${{WRKSRC}}/LICENSE

LIB_DEPENDS=	libxkbcommon.so:x11/libxkbcommon
USES=		cargo gl xorg
USE_GL=		gl
USE_XORG=	x11 xcursor xi xrandr
USE_GITHUB=	yes
GH_ACCOUNT=	grazyping
GH_PROJECT=	GrazySAI

CARGO_BUILD_ARGS=	-p paint-app
CARGO_INSTALL=	no
CARGO_TEST=	no

CARGO_CRATES=	{lines}

PLIST_FILES=	bin/GrazySAI share/applications/grazysai.desktop

do-install:
	${{INSTALL_PROGRAM}} ${{CARGO_TARGET_DIR}}/*/release/GrazySAI ${{STAGEDIR}}${{PREFIX}}/bin/GrazySAI
	${{MKDIR}} ${{STAGEDIR}}${{DESKTOPDIR}}
	${{INSTALL_DATA}} ${{WRKSRC}}/tools/freebsd/grazysai.desktop ${{STAGEDIR}}${{DESKTOPDIR}}/

.include <bsd.port.mk>
''')
open(f'{out}/pkg-descr', 'w').write('GrazySAI is a painting program in Rust modelled on PaintTool SAI 2: it opens and\nsaves .sai2 files, has the SAI brush engine, layers, linework, animation and more.\n')
print(len(crates), 'крейтов;', f'{out}')
PY
