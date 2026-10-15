# VibeTeX — Editor LaTeX Moderno, Nativo e Híbrido

> **Desenvolvido em Rust + GTK4 / Libadwaita | Linux-First | Offline-First | Lossless CST**

VibeTeX é uma alternativa open source, nativa e de alto desempenho ao Overleaf para o desktop Linux. Combina a fidelidade matemática do LaTeX com a experiência de edição visual de processadores de texto modernos e a inteligência de código de IDEs contemporâneas.

---

## 🚀 Principais Características

- **Edição Híbrida Bidirecional Real:** Edite simultaneamente em Modo Código (LaTeX), Modo Visual (Rich Text estruturado) ou Modo Dividido (*Split*).
- **Preservação Absoluta do LaTeX (Lossless CST):** O arquivo `.tex` é a fonte canônica da verdade. Comentários, preâmbulos, macros e comandos customizados são preservados bit-a-bit através de blocos protegidos e deltas pontuais baseados em intervalos de bytes (`[start, end)`).
- **Sem Conversões Destrutivas:** O documento **nunca** é convertido para HTML intermediário.
- **Visualizador PDF Nativo:** Renderização de alta fidelidade integrada via Poppler (GObject/GTK4) e Cairo, com zoom contínuo, navegação por páginas e prontidão para SyncTeX.
- **Compilação Assíncrona:** Suporte multithreaded a `latexmk`, `pdflatex`, `xelatex`, `lualatex` e `tectonic`, com timeout configurável, parser de erros/avisos com linha e coluna, e `-shell-escape` desabilitado por padrão.
- **Gerenciador Bibliográfico (.bib):** Leitura, indexação, pesquisa por autor/título/chave e inserção contextual de `\cite{...}`.
- **Catálogo de Templates Integrado:** Modelos prontos para ABNT (abnTeX2), Sociedade Brasileira de Computação (SBC), IEEE Conference, Apresentações Beamer, Artigos Acadêmicos e Documentos em Branco.
- **Importação/Exportação ZIP:** 100% compatível com pacotes `.zip` exportados pelo Overleaf, com sanitização estrita contra ataques de *path traversal* (Zip Slip).
- **Gravação Atômica e Proteção Anti-Crash:** Gravação segura em disco via arquivos temporários e renomeação atômica (`tempfile`), além de autosave em `.recovery~`.
- **Desempenho Nativo:** Binário ELF otimizado de apenas ~4.5 MB, sem Electron, sem Node.js e sem consumo excessivo de memória.

---

## 🏛️ Arquitetura Modular (Clean Architecture)

O projeto é estruturado em um workspace Cargo modular:

```text
latex/
├── Cargo.toml                    # Manifesto do workspace raiz
├── crates/
│   ├── core/                     # Spans de bytes, TextDelta, diagnósticos e eventos
│   ├── latex-parser/             # Lexer e Parser CST/AST lossless preservador de trivia
│   ├── document-model/           # Buffer canônico, patch engine, histórico de Undo/Redo
│   ├── code-editor/              # Editor de código com GtkSourceView 5 e highlight LaTeX
│   ├── visual-editor/            # Editor visual estruturado com widgets nativos GTK4
│   ├── compiler/                 # Detector de ferramentas TeX, runner assíncrono e parser de logs
│   ├── pdf-viewer/               # Visualizador de PDF com Poppler e estruturas SyncTeX
│   ├── bibliography/             # Parser .bib, busca e indexação de citações
│   ├── templates/                # Catálogo de templates (ABNT, SBC, IEEE) e engine ZIP
│   ├── workspace/                # Árvore de arquivos, detector de main.tex e I/O atômico
│   └── ui/                       # Aplicação GTK4 / Libadwaita (HeaderBar, SplitView, Drawer)
└── tests/                        # Bateria de testes de integração e conformidade
```

---

## 🛠️ Requisitos de Compilação no Linux

### Dependências de Sistema (Arch Linux / CachyOS)
```bash
sudo pacman -S gtk4 libadwaita gtksourceview5 poppler-glib pkg-config
```

### Dependências de Sistema (Ubuntu / Debian 24.04+)
```bash
sudo apt update
sudo apt install libgtk-4-dev libadwaita-1-dev libgtksourceview-5-dev libpoppler-glib-dev pkg-config
```

### Dependências de Sistema (Fedora 40+)
```bash
sudo dnf install gtk4-devel libadwaita-devel gtksourceview5-devel poppler-glib-devel pkgconf-pkg-config
```

---

## 📦 Compilação e Execução

### Executar a Suite de Testes Automatizados
```bash
cargo test --workspace
```

### Compilar a Versão de Desenvolvimento
```bash
cargo build
```

### Compilar a Versão de Release Otimizada
```bash
cargo build --release
```

### Executar o VibeTeX
```bash
# Executa abrindo o diretório atual como projeto
./target/release/vibetex

# Ou especificando o caminho de um projeto LaTeX
./target/release/vibetex /caminho/do/projeto
```

---

## 🧪 Critérios de Aceitação e Testes Validados

Todos os critérios de aceitação foram validados com 100% de sucesso através da suite de testes:

| Teste | Requisito | Resultado |
| :--- | :--- | :---: |
| **Teste 1** | Edição bidirecional (`\textbf{Original}` -> `\textbf{Modificado}`) | ✅ Passou |
| **Teste 2** | Preservação de macros, `\cite`, `\medskip` e comandos desconhecidos | ✅ Passou |
| **Teste 3** | Preservação bit-a-bit de arquivos não editados | ✅ Passou |
| **Teste 4** | Compatibilidade e integridade dos templates ABNT, SBC, IEEE, Beamer | ✅ Passou |
| **Teste 5** | Detecção de ferramentas TeX e extração de erros de log LaTeX | ✅ Passou |
| **Teste 6** | Indexação BibTeX, busca de referências e detecção de chaves ausentes | ✅ Passou |
| **Teste 8** | Isolamento de ambientes complexos e macros em `ProtectedBlock` | ✅ Passou |
| **Teste 9** | Gravação atômica em disco e geração de recuperação `.recovery~` | ✅ Passou |
| **Teste 10** | Undo/Redo atômico com restauração exata do texto canônico | ✅ Passou |
