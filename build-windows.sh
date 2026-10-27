#!/usr/bin/env bash
set -e

# ==============================================================================
# Script de Construção do Pacote Windows Portable (x86_64) do VibeTeX
# ==============================================================================

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DIST_DIR="/tmp/VibeTeX-Windows-x64"
OUTPUT_ZIP="${PROJECT_ROOT}/VibeTeX-Windows-x64.zip"

TOOLCHAIN_ROOT="${HOME}/.local/share/vibetex-toolchain"
LLVM_DIR="${TOOLCHAIN_ROOT}/llvm-mingw"
UCRT64_DIR="${TOOLCHAIN_ROOT}/msys2-ucrt64/ucrt64"
COMPAT_DIR="${TOOLCHAIN_ROOT}/llvm-mingw-compat"
TECTONIC_WIN_DIR="${TOOLCHAIN_ROOT}/tectonic-win"

mkdir -p "${TOOLCHAIN_ROOT}"

# --- Verificação e Bootstrap automático das dependências de compilação ---
if [ ! -x "${LLVM_DIR}/bin/x86_64-w64-mingw32-gcc" ]; then
    echo "==> Toolchain llvm-mingw não encontrado. Extraindo do container..."
    if ! command -v podman >/dev/null 2>&1; then
        echo "ERRO: podman é necessário para extrair o toolchain llvm-mingw." >&2
        exit 1
    fi
    podman create --name llvm_temp docker.io/mstorsjo/llvm-mingw:latest
    podman cp llvm_temp:/opt/llvm-mingw "${TOOLCHAIN_ROOT}/llvm-mingw"
    podman rm llvm_temp
fi

if [ ! -f "${COMPAT_DIR}/libgcc.a" ]; then
    echo "==> Configurando bibliotecas de compatibilidade llvm-mingw-compat..."
    mkdir -p "${COMPAT_DIR}"
    CLANG_BUILTINS="$(find "${LLVM_DIR}" -name "libclang_rt.builtins-x86_64.a" | head -n 1)"
    UNWIND_A="$(find "${LLVM_DIR}" -name "libunwind.a" | head -n 1)"
    ln -sf "${CLANG_BUILTINS}" "${COMPAT_DIR}/libgcc.a"
    ln -sf "${CLANG_BUILTINS}" "${COMPAT_DIR}/libgcc_s.a"
    ln -sf "${UNWIND_A}" "${COMPAT_DIR}/libgcc_eh.a"
    "${LLVM_DIR}/bin/llvm-ar" cr "${COMPAT_DIR}/libpthread.a"
fi

if [ ! -f "${TECTONIC_WIN_DIR}/tectonic.exe" ]; then
    echo "==> Baixando motor Tectonic para Windows..."
    mkdir -p "${TECTONIC_WIN_DIR}"
    curl -sSL "https://github.com/tectonic-typesetting/tectonic/releases/download/tectonic%400.17.0/tectonic-0.17.0-x86_64-pc-windows-msvc.zip" -o "${TECTONIC_WIN_DIR}/tectonic.zip"
    unzip -q -o "${TECTONIC_WIN_DIR}/tectonic.zip" -d "${TECTONIC_WIN_DIR}"
fi

if [ ! -d "${UCRT64_DIR}/bin" ] || [ ! -d "${UCRT64_DIR}/lib/pkgconfig" ]; then
    echo "==> Sysroot MSYS2 UCRT64 não encontrado. Baixando e extraindo runtime..."
    python3 -c "
import os, sys, tarfile, subprocess, concurrent.futures, glob, re

toolchain_dir = '${TOOLCHAIN_ROOT}'
cache_dir = os.path.join(toolchain_dir, 'cache')
sysroot_dir = os.path.join(toolchain_dir, 'msys2-ucrt64')
db_path = os.path.join(toolchain_dir, 'ucrt64.db')

os.makedirs(cache_dir, exist_ok=True)
os.makedirs(sysroot_dir, exist_ok=True)

if not os.path.exists(db_path) or os.path.getsize(db_path) == 0:
    print('Downloading ucrt64.db...')
    subprocess.check_call(['curl', '-sSL', '-A', 'Mozilla/5.0', '-o', db_path, 'https://repo.msys2.org/mingw/ucrt64/ucrt64.db'])

tar = tarfile.open(db_path)
packages = {}
for member in tar.getmembers():
    parts = member.name.split('/')
    if len(parts) == 2 and parts[1] == 'desc':
        content = tar.extractfile(member).read().decode('utf-8', errors='ignore')
        current_section = None
        data = {}
        for line in content.splitlines():
            if line.startswith('%') and line.endswith('%'):
                current_section = line[1:-1]
                data[current_section] = []
            elif line.strip() and current_section:
                data[current_section].append(line.strip())
        name = data.get('NAME', [''])[0]
        packages[name] = data
tar.close()

to_resolve = {'mingw-w64-ucrt-x86_64-gtk4', 'mingw-w64-ucrt-x86_64-libadwaita', 'mingw-w64-ucrt-x86_64-gtksourceview5', 'mingw-w64-ucrt-x86_64-poppler'}
resolved = set()

while to_resolve:
    pkg = to_resolve.pop()
    if pkg in resolved:
        continue
    pkg_data = packages.get(pkg)
    if not pkg_data:
        for k, v in packages.items():
            provides = [p.split('=')[0].split('>')[0].split('<')[0] for p in v.get('PROVIDES', [])]
            if pkg in provides or k == pkg:
                pkg_data = v
                pkg = k
                break
    if pkg_data:
        resolved.add(pkg)
        deps = pkg_data.get('DEPENDS', [])
        for dep in deps:
            dep_name = dep.split('>=')[0].split('<=')[0].split('=')[0].split('>')[0].split('<')[0]
            if dep_name not in resolved:
                to_resolve.add(dep_name)

files = [packages[p].get('FILENAME', [''])[0] for p in resolved if packages[p].get('FILENAME', [''])[0]]
print(f'Starting download and extraction of {len(files)} packages...')

base_url = 'https://repo.msys2.org/mingw/ucrt64/'

def download_and_extract(filename):
    cached_path = os.path.join(cache_dir, filename)
    if not os.path.exists(cached_path) or os.path.getsize(cached_path) == 0:
        url = base_url + filename
        res = subprocess.run(['curl', '-sSL', '-A', 'Mozilla/5.0', '-o', cached_path, url])
        if res.returncode != 0:
            raise RuntimeError(f'curl failed for {url}')
    subprocess.check_call(['tar', '--zstd', '-xf', cached_path, '-C', sysroot_dir])
    return filename

with concurrent.futures.ThreadPoolExecutor(max_workers=8) as executor:
    futures = {executor.submit(download_and_extract, fn): fn for fn in files}
    done = 0
    for future in concurrent.futures.as_completed(futures):
        fn = futures[future]
        try:
            future.result()
            done += 1
            if done % 15 == 0 or done == len(files):
                print(f'[{done}/{len(files)}] Downloaded and extracted {fn}')
        except Exception as e:
            print(f'Error processing {fn}: {e}')
            raise

print('All packages extracted successfully!')

# Patch .pc files
ucrt_path = '${UCRT64_DIR}'
pc_files = glob.glob(f'{ucrt_path}/lib/pkgconfig/*.pc') + glob.glob(f'{ucrt_path}/share/pkgconfig/*.pc')
for pc in pc_files:
    with open(pc, 'r', errors='ignore') as f:
        content = f.read()
    pattern = rf'(?<!{re.escape(sysroot_dir)})/ucrt64'
    new_content = re.sub(pattern, ucrt_path, content)
    if content != new_content:
        with open(pc, 'w') as f:
            f.write(new_content)
print(f'Patched {len(pc_files)} pc files.')
"
fi

echo "==> 1. Compilando VibeTeX para Windows (x86_64-pc-windows-gnu)..."
PATH="${LLVM_DIR}/bin:$PATH" \
PKG_CONFIG_PATH="${UCRT64_DIR}/lib/pkgconfig:${UCRT64_DIR}/share/pkgconfig" \
PKG_CONFIG_ALLOW_CROSS=1 \
CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER="${LLVM_DIR}/bin/x86_64-w64-mingw32-gcc" \
CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS="-L native=${UCRT64_DIR}/lib -L native=${COMPAT_DIR} -C link-arg=-L${UCRT64_DIR}/lib -C link-arg=-L${COMPAT_DIR} -C link-arg=-mwindows" \
cargo build --release --target x86_64-pc-windows-gnu -p vibetex-ui

echo "==> 2. Montando pasta de distribuição portátil..."
rm -rf "${DIST_DIR}"
mkdir -p "${DIST_DIR}"

# Binários principais
cp "${PROJECT_ROOT}/target/x86_64-pc-windows-gnu/release/vibetex.exe" "${DIST_DIR}/"

if [ -f "${TECTONIC_WIN_DIR}/tectonic.exe" ]; then
    cp "${TECTONIC_WIN_DIR}/tectonic.exe" "${DIST_DIR}/"
fi

# DLLs de runtime do MSYS2 UCRT64
cp "${UCRT64_DIR}/bin/"*.dll "${DIST_DIR}/"

# DLLs adicionais do toolchain llvm-mingw
for f in "${LLVM_DIR}/x86_64-w64-mingw32/bin/"*.dll; do
    [ -f "$f" ] && cp -n "$f" "${DIST_DIR}/" 2>/dev/null || true
done

# Esquemas GSettings compilados
mkdir -p "${DIST_DIR}/share/glib-2.0/schemas"
cp -r "${UCRT64_DIR}/share/glib-2.0/schemas/"* "${DIST_DIR}/share/glib-2.0/schemas/"
glib-compile-schemas "${DIST_DIR}/share/glib-2.0/schemas"

# GtkSourceView 5 e temas de ícones
mkdir -p "${DIST_DIR}/share"
cp -r "${UCRT64_DIR}/share/gtksourceview-5" "${DIST_DIR}/share/" 2>/dev/null || true
cp -r "${UCRT64_DIR}/share/icons" "${DIST_DIR}/share/" 2>/dev/null || true

# Configurações de fontes e certificados
[ -d "${UCRT64_DIR}/etc/fonts" ] && cp -r "${UCRT64_DIR}/etc/fonts" "${DIST_DIR}/etc/" 2>/dev/null || true
[ -d "${UCRT64_DIR}/etc/ssl" ] && cp -r "${UCRT64_DIR}/etc/ssl" "${DIST_DIR}/etc/" 2>/dev/null || true

# Loaders gdk-pixbuf
[ -d "${UCRT64_DIR}/lib/gdk-pixbuf-2.0" ] && cp -r "${UCRT64_DIR}/lib/gdk-pixbuf-2.0" "${DIST_DIR}/lib/" 2>/dev/null || true

# Criar LEIA-ME.txt
cat << 'EOF' > "${DIST_DIR}/LEIA-ME.txt"
================================================================================
                    VibeTeX - Editor LaTeX Moderno & Nativo
                             Versão Portátil Windows
================================================================================

COMO USAR:
1. Extraia todo o conteúdo deste arquivo .zip para qualquer pasta do seu computador
   (por exemplo: C:\VibeTeX ou na sua Área de Trabalho).
2. Dê um duplo-clique no executável:
   -> vibetex.exe

RECURSOS E VANTAGENS DESTA VERSÃO:
- Totalmente portátil (Portable): Não requer instalação nem permissão de administrador.
- Compilador LaTeX Integrado: Já inclui o motor Tectonic (tectonic.exe) embutido.
  Você não precisa instalar TeX Live nem MiKTeX! O VibeTeX detecta e compila
  seus documentos LaTeX automaticamente.
- Interface Nativa Moderna: Construído com Libadwaita & GTK4, com suporte a tema
  escuro e claro, abas, editor de código com realce de sintaxe e visualizador de PDF
  integrado lado a lado.
- Seleção e Cópia no PDF: Suporte aprimorado para seleção e visualização fluida de documentos.

ATALHOS ÚTEIS:
- Ctrl + S: Salvar arquivo atual
- Ctrl + B / F5: Compilar documento LaTeX
- Ctrl + N: Novo documento
- Ctrl + O: Abrir projeto ou arquivo .tex
- F11: Alternar tela cheia

Dúvidas ou feedbacks para melhorias são super bem-vindos!
================================================================================
EOF

echo "==> 3. Compactando pacote ZIP..."
python3 -c "
import os, zipfile
source_dir = '${DIST_DIR}'
output_zip = '${OUTPUT_ZIP}'
with zipfile.ZipFile(output_zip, 'w', compression=zipfile.ZIP_DEFLATED, compresslevel=6) as zipf:
    for root, dirs, files in os.walk(source_dir):
        for file in files:
            full_path = os.path.join(root, file)
            arcname = os.path.relpath(full_path, '/tmp')
            zipf.write(full_path, arcname)
"

echo "==> Pacote Windows gerado com sucesso em: ${OUTPUT_ZIP}"
ls -lh "${OUTPUT_ZIP}"
