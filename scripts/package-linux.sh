#!/usr/bin/env bash
# Gera .deb, .rpm e .AppImage a partir do JAR já compilado (ant jar).
# O app é um JAR puro: .deb "all" e .rpm "noarch"; só o AppImage é x86_64 (runtime).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

VERSION="${VERSION:-$(grep -oP 'name="version" value="\K[^"]+' build.xml | head -1)}"
JAR="build/jar/guialar-digital.jar"
ICON="packaging/guialar-digital.svg"
OUT="$ROOT/dist/linux"
APP_NAME="guialar-digital"
DISPLAY_NAME="GuiaLar Digital"
DESCRICAO="Ativa um filtro de DNS para a família e orienta a proteção dos navegadores"

if [ ! -f "$JAR" ]; then
  echo "JAR não encontrado em $JAR. Rode 'ant package-linux' (ele compila antes)." >&2
  exit 1
fi

rm -rf "$OUT"
mkdir -p "$OUT"

make_desktop() {
  cat > "$1" <<DESKTOP
[Desktop Entry]
Type=Application
Name=${DISPLAY_NAME}
Comment=${DESCRICAO}
Exec=${APP_NAME}
Icon=${APP_NAME}
Terminal=false
Categories=Network;Security;
Keywords=DNS;família;filtro;privacidade;
StartupNotify=true
DESKTOP
}

make_wrapper() {
  cat > "$1" <<WRAPPER
#!/bin/sh
exec java -jar "$2" "\$@"
WRAPPER
  chmod 755 "$1"
}

# Mesma árvore de arquivos para .deb e .rpm
stage() {
  local root="$1"
  mkdir -p "$root/usr/bin" "$root/usr/share/${APP_NAME}" "$root/usr/share/applications" \
    "$root/usr/share/icons/hicolor/scalable/apps"
  install -m 644 "$JAR" "$root/usr/share/${APP_NAME}/guialar-digital.jar"
  make_wrapper "$root/usr/bin/${APP_NAME}" "/usr/share/${APP_NAME}/guialar-digital.jar"
  make_desktop "$root/usr/share/applications/${APP_NAME}.desktop"
  install -m 644 "$ICON" "$root/usr/share/icons/hicolor/scalable/apps/${APP_NAME}.svg"
}

# ─── .deb ────────────────────────────────────────────────────
echo "==> .deb"
DEB_ROOT="$OUT/deb-root"
stage "$DEB_ROOT"
mkdir -p "$DEB_ROOT/DEBIAN" "$DEB_ROOT/usr/share/doc/${APP_NAME}"
install -m 644 README.md "$DEB_ROOT/usr/share/doc/${APP_NAME}/README.md"
cat > "$DEB_ROOT/DEBIAN/control" <<CONTROL
Package: ${APP_NAME}
Version: ${VERSION}
Section: net
Priority: optional
Architecture: all
Depends: openjdk-17-jre | openjdk-21-jre | java17-runtime
Recommends: pkexec | policykit-1
Maintainer: GuiaLar Digital <https://github.com/Guilherme4Colamarco/guialar-digital>
Homepage: https://github.com/Guilherme4Colamarco/guialar-digital
Description: ${DESCRICAO}
 Configura o DNS do sistema para o Cloudflare for Families (bloqueia malware
 e conteúdo adulto), com opção de desfazer, e guia a proteção dos navegadores.
CONTROL
DEB_FILE="$OUT/${APP_NAME}_${VERSION}_all.deb"
dpkg-deb --build --root-owner-group "$DEB_ROOT" "$DEB_FILE" >/dev/null
rm -rf "$DEB_ROOT"
echo "    $DEB_FILE"

# ─── .rpm ────────────────────────────────────────────────────
echo "==> .rpm"
if command -v rpmbuild >/dev/null 2>&1; then
  RPM_TOP="$OUT/rpmbuild"
  mkdir -p "$RPM_TOP"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}
  STAGE="$OUT/rpm-stage"
  stage "$STAGE"
  tar -C "$STAGE" -czf "$RPM_TOP/SOURCES/${APP_NAME}-${VERSION}.tar.gz" .
  cat > "$RPM_TOP/SPECS/${APP_NAME}.spec" <<SPEC
Name:           ${APP_NAME}
Version:        ${VERSION}
Release:        1
Summary:        ${DESCRICAO}
License:        Proprietary
URL:            https://github.com/Guilherme4Colamarco/guialar-digital
Source0:        %{name}-%{version}.tar.gz
BuildArch:      noarch
Requires:       (java-17-openjdk or java-21-openjdk or java-latest-openjdk)
Recommends:     polkit

%description
Configura o DNS do sistema para o Cloudflare for Families (bloqueia malware
e conteúdo adulto), com opção de desfazer, e guia a proteção dos navegadores.

%prep
%setup -c -n %{name}-%{version}

%install
mkdir -p %{buildroot}
cp -a . %{buildroot}/

%files
/usr/bin/${APP_NAME}
/usr/share/${APP_NAME}/guialar-digital.jar
/usr/share/applications/${APP_NAME}.desktop
/usr/share/icons/hicolor/scalable/apps/${APP_NAME}.svg
SPEC
  rpmbuild --quiet --define "_topdir $RPM_TOP" -bb "$RPM_TOP/SPECS/${APP_NAME}.spec"
  RPM_FILE="$OUT/${APP_NAME}-${VERSION}-1.noarch.rpm"
  cp "$RPM_TOP/RPMS/noarch/"*.rpm "$RPM_FILE"
  rm -rf "$RPM_TOP" "$STAGE"
  echo "    $RPM_FILE"
else
  echo "    AVISO: rpmbuild ausente, .rpm não gerado (instale o pacote 'rpm')."
fi

# ─── AppImage ────────────────────────────────────────────────
echo "==> AppImage"
APPDIR="$OUT/${APP_NAME}.AppDir"
mkdir -p "$APPDIR/usr/share/${APP_NAME}"
install -m 644 "$JAR" "$APPDIR/usr/share/${APP_NAME}/guialar-digital.jar"
cat > "$APPDIR/AppRun" <<'APPRUN'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
exec java -jar "$HERE/usr/share/guialar-digital/guialar-digital.jar" "$@"
APPRUN
chmod 755 "$APPDIR/AppRun"
make_desktop "$APPDIR/${APP_NAME}.desktop"
install -m 644 "$ICON" "$APPDIR/${APP_NAME}.svg"
ln -sf "${APP_NAME}.svg" "$APPDIR/.DirIcon"

APPIMAGE_FILE="$OUT/${APP_NAME}-${VERSION}-x86_64.AppImage"
if command -v appimagetool >/dev/null 2>&1; then
  ARCH=x86_64 appimagetool --no-appstream "$APPDIR" "$APPIMAGE_FILE" >/dev/null
  rm -rf "$APPDIR"
  echo "    $APPIMAGE_FILE"
else
  TAR="$OUT/${APP_NAME}-${VERSION}-AppDir.tar.gz"
  tar -C "$OUT" -czf "$TAR" "$(basename "$APPDIR")"
  rm -rf "$APPDIR"
  echo "    AVISO: appimagetool ausente, AppDir empacotado em $TAR"
fi

echo
ls -lh "$OUT"
