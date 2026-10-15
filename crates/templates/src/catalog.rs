use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Template {
    pub id: &'static str,
    pub name: &'static str,
    pub category: &'static str,
    pub description: &'static str,
    pub files: HashMap<&'static str, &'static str>,
}

impl Template {
    pub fn instantiate(&self, target_dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(target_dir)?;
        for (rel_path, content) in &self.files {
            let file_path = target_dir.join(rel_path);
            if let Some(parent) = file_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&file_path, content)?;
        }
        Ok(())
    }
}

pub struct TemplateCatalog;

impl TemplateCatalog {
    pub fn all() -> Vec<Template> {
        vec![
            Self::blank(),
            Self::academic_article(),
            Self::abnt_article(),
            Self::sbc_article(),
            Self::ieee_paper(),
            Self::beamer_presentation(),
        ]
    }

    pub fn get(id: &str) -> Option<Template> {
        Self::all().into_iter().find(|t| t.id == id)
    }

    fn blank() -> Template {
        let mut files = HashMap::new();
        files.insert(
            "main.tex",
            r"\documentclass[12pt]{article}
\usepackage[utf8]{inputenc}
\usepackage[T1]{fontenc}
\usepackage[brazil]{babel}

\title{Novo Documento}
\author{Autor}
\date{\today}

\begin{document}
\maketitle

\section{Introdução}
Comece a escrever seu documento LaTeX aqui.

\end{document}
",
        );

        Template {
            id: "blank",
            name: "Documento em Branco",
            category: "Geral",
            description: "Template limpo e direto para novos documentos.",
            files,
        }
    }

    fn academic_article() -> Template {
        let mut files = HashMap::new();
        files.insert(
            "main.tex",
            r"\documentclass[11pt,a4paper]{article}
\usepackage[utf8]{inputenc}
\usepackage[T1]{fontenc}
\usepackage{amsmath,amssymb}
\usepackage{graphicx}
\usepackage{hyperref}

\title{Título do Artigo Científico}
\author{Pesquisador Principal \and Coautor}
\date{\today}

\begin{document}
\maketitle

\begin{abstract}
Este é o resumo conciso do artigo acadêmico, sintetizando a motivação, metodologia e conclusões obtidas.
\end{abstract}

\section{Introdução}
A evolução dos editores LaTeX nativos permite uma integração semântica sem perdas \cite{knuth1984}.

\section{Metodologia}
A modelagem bidirecional baseia-se em spans exatos de bytes no código canônico.

\begin{equation}
E = mc^2
\end{equation}

\section{Resultados}
Os testes demonstram latência imperceptível e fidelidade 100\% nos comandos LaTeX.

\bibliographystyle{plain}
\bibliography{references}

\end{document}
",
        );
        files.insert(
            "references.bib",
            r"@article{knuth1984,
  author = {Donald E. Knuth},
  title = {Literate Programming},
  journal = {The Computer Journal},
  year = {1984},
  volume = {27},
  pages = {97--111}
}
",
        );

        Template {
            id: "article",
            name: "Artigo Acadêmico",
            category: "Acadêmico",
            description: "Estrutura completa com resumo, equações e bibliografia BibTeX.",
            files,
        }
    }

    fn abnt_article() -> Template {
        let mut files = HashMap::new();
        files.insert(
            "main.tex",
            r"\documentclass[12pt,oneside,a4paper]{abntex2}
\usepackage[utf8]{inputenc}
\usepackage[T1]{fontenc}
\usepackage[brazil]{babel}
\usepackage{microtype}
\usepackage{amsmath}

\titulo{Título do Trabalho Acadêmico em Conformidade com a ABNT}
\autor{Nome do Estudante}
\local{Brasil}
\data{\today}

\begin{document}

\maketitle

\begin{resumo}
Este modelo segue as diretrizes da Associação Brasileira de Normas Técnicas (ABNT) utilizando o pacote abnTeX2.
\textbf{Palavras-chave}: LaTeX. ABNT. Edição Científica.
\end{resumo}

\section{Introdução}
O presente trabalho apresenta uma abordagem nativa e offline-first para escrita acadêmica no Brasil \cite{silva2026}.

\section{Desenvolvimento}
A utilização correta das normas ABNT garante a padronização e o rigor metodológico.

\bibliography{referencias}

\end{document}
",
        );
        files.insert(
            "referencias.bib",
            r"@article{silva2026,
  author = {Silva, Lucas e Santos, Mariana},
  title = {Avanços na Automação Editorial Científica},
  journal = {Revista Brasileira de Computação},
  year = {2026},
  volume = {14},
  pages = {45--60}
}
",
        );

        Template {
            id: "abnt",
            name: "Normas ABNT (abnTeX2)",
            category: "ABNT / Brasil",
            description: "Modelo para artigos, TCCs e relatórios formatados nas normas ABNT.",
            files,
        }
    }

    fn sbc_article() -> Template {
        let mut files = HashMap::new();
        files.insert(
            "main.tex",
            r"\documentclass[12pt]{article}
\usepackage{sbc-template}
\usepackage{graphicx,url}
\usepackage[brazil]{babel}
\usepackage[utf8]{inputenc}

\title{Instruções para Autores de Artigos da SBC}
\author{Primeiro Autor\inst{1}, Segundo Autor\inst{2}}
\address{Instituto de Computação -- Universidade Federal (UF) \and Departamento de Ciência da Computação}

\begin{document}
\maketitle

\begin{abstract}
This meta-paper describes the style to be used in articles for SBC conferences.
\end{abstract}

\begin{resumo}
Este documento descreve o formato para submissões em eventos da Sociedade Brasileira de Computação (SBC).
\end{resumo}

\section{Introdução}
Os artigos submetidos devem ter no máximo 12 páginas em formato PDF.

\end{document}
",
        );

        Template {
            id: "sbc",
            name: "SBC (Soc. Brasileira de Computação)",
            category: "Conferências",
            description: "Modelo oficial para submissões em conferências e simpósios da SBC.",
            files,
        }
    }

    fn ieee_paper() -> Template {
        let mut files = HashMap::new();
        files.insert(
            "main.tex",
            r"\documentclass[conference]{IEEEtran}
\usepackage{cite}
\usepackage{amsmath,amssymb,amsfonts}
\usepackage{graphicx}

\begin{document}
\title{High-Performance Native LaTeX Architecture in Rust}
\author{\IEEEauthorblockN{Author Name}
\IEEEauthorblockA{\textit{Department of Computer Science} \\
\textit{University}\\
City, Country}}

\maketitle

\begin{abstract}
This paper presents a desktop LaTeX environment combining rich visual editing with concrete syntax tree source synchronization.
\end{abstract}

\section{Introduction}
Modern engineering environments require high fidelity and offline capabilities.

\section{Conclusion}
The proposed architecture provides seamless bidirectional workflows.

\end{document}
",
        );

        Template {
            id: "ieee",
            name: "IEEE Conference (2 Colunas)",
            category: "Conferências",
            description: "Modelo tradicional em duas colunas da IEEE para publicações internacionais.",
            files,
        }
    }

    fn beamer_presentation() -> Template {
        let mut files = HashMap::new();
        files.insert(
            "main.tex",
            r"\documentclass{beamer}
\usetheme{Madrid}
\usecolortheme{default}

\title{Apresentação Acadêmica}
\subtitle{Demonstração de Recursos no VibeTeX}
\author{Apresentador}
\date{\today}

\begin{document}

\begin{frame}
\titlepage
\end{frame}

\begin{frame}{Roteiro}
\tableofcontents
\end{frame}

\section{Introdução}
\begin{frame}{Objetivo}
\begin{itemize}
  \item Edição visual rápida e intuitiva
  \item Preservação absoluta do código LaTeX
  \item Compilação rápida e nativa no Linux
\end{itemize}
\end{frame}

\section{Demonstração}
\begin{frame}{Exemplo de Código e Equação}
A relação fundamental:
\begin{equation}
e^{i\pi} + 1 = 0
\end{equation}
\end{frame}

\end{document}
",
        );

        Template {
            id: "beamer",
            name: "Apresentação Beamer (Slides)",
            category: "Apresentações",
            description: "Apresentação de slides elegante com tema Madrid.",
            files,
        }
    }
}
