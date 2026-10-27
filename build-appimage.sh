#!/usr/bin/env bash
set -e

# ==============================================================================
# Script de Construção do AppImage do VibeTeX
# ==============================================================================

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APPDIR="/tmp/vibetex-appdir"
OUTPUT_APPIMAGE="${PROJECT_ROOT}/VibeTeX-x86_64.AppImage"

echo "==> 1. Compilando VibeTeX em modo Release..."
cargo build --release -p vibetex-ui

echo "==> 2. Preparando estrutura do AppDir..."
rm -rf "${APPDIR}"
mkdir -p "${APPDIR}/usr/bin"
mkdir -p "${APPDIR}/usr/lib"
mkdir -p "${APPDIR}/usr/share/applications"
mkdir -p "${APPDIR}/usr/share/icons/hicolor/256x256/apps"
mkdir -p "${APPDIR}/usr/share/glib-2.0/schemas"

# Copiar binário principal
cp "${PROJECT_ROOT}/target/release/vibetex" "${APPDIR}/usr/bin/"

# Copiar tectonic se disponível
if [ -f "${HOME}/.local/bin/tectonic" ]; then
    echo "==> Incluindo compilador Tectonic embutido..."
    cp "${HOME}/.local/bin/tectonic" "${APPDIR}/usr/bin/"
elif command -v tectonic >/dev/null 2>&1; then
    echo "==> Incluindo compilador Tectonic embutido..."
    cp "$(command -v tectonic)" "${APPDIR}/usr/bin/"
fi

# Copiar especificações de sintaxe do GtkSourceView 5
if [ -d "/usr/share/gtksourceview-5" ]; then
    cp -r /usr/share/gtksourceview-5 "${APPDIR}/usr/share/"
fi

# Copiar e compilar esquemas GSettings
cp /usr/share/glib-2.0/schemas/*gtk* "${APPDIR}/usr/share/glib-2.0/schemas/" 2>/dev/null || true
cp /usr/share/glib-2.0/schemas/*adw* "${APPDIR}/usr/share/glib-2.0/schemas/" 2>/dev/null || true
cp /usr/share/glib-2.0/schemas/*gtksourceview* "${APPDIR}/usr/share/glib-2.0/schemas/" 2>/dev/null || true
glib-compile-schemas "${APPDIR}/usr/share/glib-2.0/schemas"

# Copiar temas de ícones padrão
if [ -d "/usr/share/icons/hicolor" ]; then
    cp -r /usr/share/icons/hicolor "${APPDIR}/usr/share/icons/"
fi
if [ -d "/usr/share/icons/Adwaita" ]; then
    cp -r /usr/share/icons/Adwaita "${APPDIR}/usr/share/icons/" 2>/dev/null || true
fi

# Criar .desktop
cat << 'EOF' > "${APPDIR}/vibetex.desktop"
[Desktop Entry]
Type=Application
Name=VibeTeX
GenericName=Editor LaTeX
Comment=Editor LaTeX Moderno e Nativo com Visualização PDF em Tempo Real
Exec=vibetex %U
Icon=vibetex
Terminal=false
Categories=Office;Development;Publishing;
MimeType=text/x-tex;application/x-latex;
StartupWMClass=vibetex
Keywords=latex;tex;editor;pdf;bibtex;tectonic;
EOF
cp "${APPDIR}/vibetex.desktop" "${APPDIR}/usr/share/applications/vibetex.desktop"

# Copiar ícone
ICON_PATH="${PROJECT_ROOT}/resources/icons/vibetex.png"
if [ -f "${ICON_PATH}" ]; then
    cp "${ICON_PATH}" "${APPDIR}/vibetex.png"
    cp "${ICON_PATH}" "${APPDIR}/usr/share/icons/hicolor/256x256/apps/vibetex.png"
    cp "${ICON_PATH}" "${APPDIR}/.DirIcon"
fi

# Criar script AppRun inicializador
cat << 'EOF' > "${APPDIR}/AppRun"
#!/bin/bash
set -e

HERE="$(dirname "$(readlink -f "${0}")")"

export APPDIR="${HERE}"
export PATH="${HERE}/usr/bin:${PATH}"
export LD_LIBRARY_PATH="${HERE}/usr/lib:${LD_LIBRARY_PATH}"
export XDG_DATA_DIRS="${HERE}/usr/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"
export GSETTINGS_SCHEMA_DIR="${HERE}/usr/share/glib-2.0/schemas:${GSETTINGS_SCHEMA_DIR}"

if [ -d "${HERE}/usr/lib/gtk-4.0" ]; then
    export GTK_PATH="${HERE}/usr/lib/gtk-4.0:${GTK_PATH}"
fi

exec "${HERE}/usr/bin/vibetex" "$@"
EOF
chmod +x "${APPDIR}/AppRun"

LINUXDEPLOY="$(command -v linuxdeploy 2>/dev/null || echo "${HOME}/.local/bin/linuxdeploy")"
if [ ! -x "${LINUXDEPLOY}" ]; then
    echo "==> Baixando linuxdeploy..."
    curl -sSL -o "${HOME}/.local/bin/linuxdeploy" "https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage"
    chmod +x "${HOME}/.local/bin/linuxdeploy"
    LINUXDEPLOY="${HOME}/.local/bin/linuxdeploy"
fi

echo "==> 3. Empacotando dependências com linuxdeploy..."
"${LINUXDEPLOY}" --appdir "${APPDIR}" -e "${APPDIR}/usr/bin/vibetex" -d "${APPDIR}/vibetex.desktop" -i "${APPDIR}/vibetex.png"

# Restaurar AppRun customizado caso linuxdeploy sobrescreva com symlink
if [ -L "${APPDIR}/AppRun" ]; then
    rm -f "${APPDIR}/AppRun"
    cat << 'EOF' > "${APPDIR}/AppRun"
#!/bin/bash
set -e

HERE="$(dirname "$(readlink -f "${0}")")"

export APPDIR="${HERE}"
export PATH="${HERE}/usr/bin:${PATH}"
export LD_LIBRARY_PATH="${HERE}/usr/lib:${LD_LIBRARY_PATH}"
export XDG_DATA_DIRS="${HERE}/usr/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"
export GSETTINGS_SCHEMA_DIR="${HERE}/usr/share/glib-2.0/schemas:${GSETTINGS_SCHEMA_DIR}"

if [ -d "${HERE}/usr/lib/gtk-4.0" ]; then
    export GTK_PATH="${HERE}/usr/lib/gtk-4.0:${GTK_PATH}"
fi

exec "${HERE}/usr/bin/vibetex" "$@"
EOF
    chmod +x "${APPDIR}/AppRun"
fi

echo "==> 4. Gerando arquivo AppImage final..."
ARCH=x86_64 appimagetool "${APPDIR}" "${OUTPUT_APPIMAGE}"

echo "==> Sucesso! AppImage gerado em: ${OUTPUT_APPIMAGE}"
ls -lh "${OUTPUT_APPIMAGE}"
