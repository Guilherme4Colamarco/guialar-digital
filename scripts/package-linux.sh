#!/usr/bin/env bash
# Gera .deb, .rpm e .AppImage do GuiaLar Digital (depende de Java 17+ no sistema).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

VERSION="${VERSION:-$(grep -oP 'name="version" value="\K[^"]+' build.xml | head -1)}"
VERSION="${VERSION:-0.3.0}"
ARCH="${ARCH:-amd64}"
RPM_ARCH="${RPM_ARCH:-x86_64}"
JAR="build/jar/guialar-digital.jar"
OUT="$ROOT/dist/linux"
APP_NAME="guialar-digital"
DISPLAY_NAME="GuiaLar Digital"

echo "==> Compilando JAR..."
ant -q jar
test -f "$JAR"

rm -rf "$OUT"
mkdir -p "$OUT"

# ─── Desktop + wrapper ───────────────────────────────────────
make_desktop() {
  local dest="$1"
  cat > "$dest" <<EOF
[Desktop Entry]
Type=Application
Name=${DISPLAY_NAME}
Comment=DNS seguro e adblockers (Linux + Windows)
Exec=${APP_NAME}
Icon=${APP_NAME}
Terminal=false
Categories=Network;Security;
Keywords=DNS;uBlock;privacy;
StartupNotify=true
EOF
}

make_wrapper() {
  local dest="$1"
  local jar_path="$2"
  cat > "$dest" <<EOF
#!/bin/sh
exec java -jar "${jar_path}" "\$@"
EOF
  chmod +x "$dest"
}

# ─── .deb ────────────────────────────────────────────────────
echo "==> Empacotando .deb..."
DEB_ROOT="$OUT/deb-root"
mkdir -p "$DEB_ROOT/DEBIAN" \
  "$DEB_ROOT/usr/bin" \
  "$DEB_ROOT/usr/share/${APP_NAME}" \
  "$DEB_ROOT/usr/share/applications" \
  "$DEB_ROOT/usr/share/doc/${APP_NAME}"

cp "$JAR" "$DEB_ROOT/usr/share/${APP_NAME}/"
make_wrapper "$DEB_ROOT/usr/bin/${APP_NAME}" "/usr/share/${APP_NAME}/guialar-digital.jar"
make_desktop "$DEB_ROOT/usr/share/applications/${APP_NAME}.desktop"
cp README.md "$DEB_ROOT/usr/share/doc/${APP_NAME}/" 2>/dev/null || true

cat > "$DEB_ROOT/DEBIAN/control" <<EOF
Package: ${APP_NAME}
Version: ${VERSION}
Section: net
Priority: optional
Architecture: ${ARCH}
Depends: openjdk-17-jre | java17-runtime | java-runtime (>= 17)
Maintainer: Guilherme Amaral <guialar@local>
Description: Assistente para DNS seguro e adblockers
 GuiaLar Digital configura DNS do sistema (Cloudflare Families ou
 uBlockDNS) e orienta instalação de adblockers em Linux e Windows.
EOF

DEB_FILE="$OUT/${APP_NAME}_${VERSION}_${ARCH}.deb"
dpkg-deb --build --root-owner-group "$DEB_ROOT" "$DEB_FILE"
echo "    OK $DEB_FILE"

# ─── .rpm (via rpmbuild se disponível; senão alien) ──────────
echo "==> Empacotando .rpm..."
RPM_FILE="$OUT/${APP_NAME}-${VERSION}-1.${RPM_ARCH}.rpm"
if command -v rpmbuild >/dev/null 2>&1; then
  RPM_TOP="$OUT/rpmbuild"
  mkdir -p "$RPM_TOP"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}
  STAGE="$OUT/rpm-stage"
  rm -rf "$STAGE"
  mkdir -p "$STAGE/usr/bin" "$STAGE/usr/share/${APP_NAME}" "$STAGE/usr/share/applications"
  cp "$JAR" "$STAGE/usr/share/${APP_NAME}/"
  make_wrapper "$STAGE/usr/bin/${APP_NAME}" "/usr/share/${APP_NAME}/guialar-digital.jar"
  make_desktop "$STAGE/usr/share/applications/${APP_NAME}.desktop"
  tar -C "$STAGE" -czf "$RPM_TOP/SOURCES/${APP_NAME}-${VERSION}.tar.gz" .
  cat > "$RPM_TOP/SPECS/${APP_NAME}.spec" <<EOF
Name:           ${APP_NAME}
Version:        ${VERSION}
Release:        1%{?dist}
Summary:        Assistente para DNS seguro e adblockers
License:        Proprietary
URL:            https://github.com/Guilherme4Colamarco/guialar-digital
Source0:        %{name}-%{version}.tar.gz
BuildArch:      ${RPM_ARCH}
Requires:       java-17-openjdk >= 1:17

%description
GuiaLar Digital — DNS do sistema (Families / uBlockDNS) e adblockers.

%prep
%setup -c -n %{name}-%{version}

%install
rm -rf %{buildroot}
mkdir -p %{buildroot}
cp -a . %{buildroot}/

%files
/usr/bin/${APP_NAME}
/usr/share/${APP_NAME}/guialar-digital.jar
/usr/share/applications/${APP_NAME}.desktop
EOF
  rpmbuild --define "_topdir $RPM_TOP" -bb "$RPM_TOP/SPECS/${APP_NAME}.spec"
  find "$RPM_TOP/RPMS" -name '*.rpm' -exec cp {} "$RPM_FILE" \;
  echo "    OK $RPM_FILE"
elif command -v alien >/dev/null 2>&1; then
  (cd "$OUT" && alien -r --scripts "$DEB_FILE")
  echo "    OK (via alien) em $OUT"
else
  echo "    AVISO: rpmbuild/alien ausentes — .rpm não gerado"
fi

# ─── AppImage (AppDir + appimagetool se disponível) ──────────
echo "==> Empacotando AppImage..."
APPDIR="$OUT/${DISPLAY_NAME// /_}.AppDir"
rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/share/${APP_NAME}" "$APPDIR/usr/share/applications"
cp "$JAR" "$APPDIR/usr/share/${APP_NAME}/"
make_wrapper "$APPDIR/usr/bin/${APP_NAME}" "usr/share/${APP_NAME}/guialar-digital.jar"
# AppRun relativo ao AppDir
cat > "$APPDIR/AppRun" <<'EOF'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
exec java -jar "$HERE/usr/share/guialar-digital/guialar-digital.jar" "$@"
EOF
chmod +x "$APPDIR/AppRun"
make_desktop "$APPDIR/${APP_NAME}.desktop"
# Ícone placeholder (necessário para appimagetool)
printf 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==' | base64 -d > "$APPDIR/${APP_NAME}.png" 2>/dev/null || \
  printf '\x89PNG\r\n\x1a\n' > "$APPDIR/${APP_NAME}.png"
ln -sf "${APP_NAME}.png" "$APPDIR/.DirIcon" 2>/dev/null || true
sed -i "s|^Icon=.*|Icon=${APP_NAME}|" "$APPDIR/${APP_NAME}.desktop"
cp "$APPDIR/${APP_NAME}.desktop" "$APPDIR/usr/share/applications/"

APPIMAGE_FILE="$OUT/${APP_NAME}-${VERSION}-${ARCH}.AppImage"
if command -v appimagetool >/dev/null 2>&1; then
  ARCH=x86_64 appimagetool "$APPDIR" "$APPIMAGE_FILE"
  echo "    OK $APPIMAGE_FILE"
else
  # Fallback: tarball do AppDir (usuário pode gerar AppImage depois)
  TAR="$OUT/${APP_NAME}-${VERSION}-${ARCH}-AppDir.tar.gz"
  tar -C "$OUT" -czf "$TAR" "$(basename "$APPDIR")"
  echo "    AVISO: appimagetool ausente — AppDir empacotado em $TAR"
  echo "    Baixe appimagetool e rode: appimagetool \"$APPDIR\" \"$APPIMAGE_FILE\""
fi

echo
echo "==> Artefatos em $OUT"
ls -lh "$OUT"/*.{deb,rpm,AppImage,tar.gz} 2>/dev/null || ls -lh "$OUT"
