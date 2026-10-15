#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionKind {
    Command,
    Citation,
    Environment,
    Label,
    Math,
}

impl CompletionKind {
    pub fn icon_name(&self) -> &'static str {
        match self {
            CompletionKind::Command => "system-run-symbolic",
            CompletionKind::Citation => "x-office-address-book-symbolic",
            CompletionKind::Environment => "view-grid-symbolic",
            CompletionKind::Label => "tag-symbolic",
            CompletionKind::Math => "accessories-calculator-symbolic",
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            CompletionKind::Command => "Comando",
            CompletionKind::Citation => "Citação",
            CompletionKind::Environment => "Ambiente",
            CompletionKind::Label => "Rótulo",
            CompletionKind::Math => "Matemática",
        }
    }
}

#[derive(Debug, Clone)]
pub struct CompletionItem {
    pub label: String,
    pub insert_text: String,
    pub detail: String,
    pub kind: CompletionKind,
    /// Number of bytes to replace before cursor (e.g. length of typed prefix)
    pub replace_len: usize,
    /// Offset to rewind cursor backwards inside delimiters (e.g. 1 for inside `{}`)
    pub cursor_rewind: usize,
}

#[derive(Debug, Clone)]
pub struct BibSuggestion {
    pub cite_key: String,
    pub author: String,
    pub year: String,
    pub title: String,
}

pub struct CompletionEngine;

impl CompletionEngine {
    /// Queries matching completion items based on the text before cursor.
    pub fn query(
        source_before_cursor: &str,
        bib_suggestions: &[BibSuggestion],
        labels: &[String],
    ) -> Vec<CompletionItem> {
        let bytes = source_before_cursor.as_bytes();
        if bytes.is_empty() {
            return Vec::new();
        }

        // 1. Check if inside \cite{...
        if let Some(brace_pos) = source_before_cursor.rfind(r"\cite{") {
            let after_brace = &source_before_cursor[brace_pos + 6..];
            if !after_brace.contains('}') && !after_brace.contains('\n') {
                let query = after_brace.trim().to_lowercase();
                return bib_suggestions
                    .iter()
                    .filter(|b| {
                        query.is_empty()
                            || b.cite_key.to_lowercase().contains(&query)
                            || b.author.to_lowercase().contains(&query)
                            || b.title.to_lowercase().contains(&query)
                    })
                    .map(|b| CompletionItem {
                        label: b.cite_key.clone(),
                        insert_text: format!("{}}}", b.cite_key),
                        detail: format!("{} ({}) - {}", b.author, b.year, b.title),
                        kind: CompletionKind::Citation,
                        replace_len: after_brace.len(),
                        cursor_rewind: 0,
                    })
                    .take(15)
                    .collect();
            }
        }

        // 2. Check if inside \ref{...
        if let Some(brace_pos) = source_before_cursor.rfind(r"\ref{") {
            let after_brace = &source_before_cursor[brace_pos + 5..];
            if !after_brace.contains('}') && !after_brace.contains('\n') {
                let query = after_brace.trim().to_lowercase();
                return labels
                    .iter()
                    .filter(|l| query.is_empty() || l.to_lowercase().contains(&query))
                    .map(|l| CompletionItem {
                        label: l.clone(),
                        insert_text: format!("{}}}", l),
                        detail: "Referência cruzada".to_string(),
                        kind: CompletionKind::Label,
                        replace_len: after_brace.len(),
                        cursor_rewind: 0,
                    })
                    .take(15)
                    .collect();
            }
        }

        // 3. Check if inside \begin{...
        if let Some(brace_pos) = source_before_cursor.rfind(r"\begin{") {
            let after_brace = &source_before_cursor[brace_pos + 7..];
            if !after_brace.contains('}') && !after_brace.contains('\n') {
                let query = after_brace.trim().to_lowercase();
                let environments = [
                    ("equation", "Equação matemática numerada", "equation}\n  \n\\end{equation}", 18),
                    ("align", "Alinhamento de equações com &", "align}\n  \n\\end{align}", 15),
                    ("gather", "Grupo de equações consecutivas", "gather}\n  \n\\end{gather}", 16),
                    ("itemize", "Lista com marcadores", "itemize}\n  \\item \n\\end{itemize}", 14),
                    ("enumerate", "Lista numerada sequencial", "enumerate}\n  \\item \n\\end{enumerate}", 16),
                    ("figure", "Figura com legenda e rótulo", "figure}[htbp]\n  \\centering\n  \\includegraphics[width=0.8\\textwidth]{}\n  \\caption{}\n  \\label{fig:}\n\\end{figure}", 25),
                    ("table", "Tabela formatada com legenda", "table}[htbp]\n  \\centering\n  \\begin{tabular}{cc}\n    \n  \\end{tabular}\n  \\caption{}\n  \\label{tab:}\n\\end{table}", 35),
                    ("abstract", "Resumo do documento", "abstract}\n  \n\\end{abstract}", 18),
                    ("verbatim", "Texto literal monoespaçado", "verbatim}\n  \n\\end{verbatim}", 18),
                    ("center", "Centralização de parágrafo", "center}\n  \n\\end{center}", 16),
                ];

                return environments
                    .iter()
                    .filter(|(env, _, _, _)| query.is_empty() || env.starts_with(&query))
                    .map(|(env, desc, template, rewind)| CompletionItem {
                        label: env.to_string(),
                        insert_text: template.to_string(),
                        detail: desc.to_string(),
                        kind: CompletionKind::Environment,
                        replace_len: after_brace.len(),
                        cursor_rewind: *rewind,
                    })
                    .collect();
            }
        }

        // 4. Check if typing a macro starting with backslash '\'
        let mut i = bytes.len();
        while i > 0 && bytes[i - 1].is_ascii_alphabetic() {
            i -= 1;
        }

        if i > 0 && bytes[i - 1] == b'\\' {
            let prefix = &source_before_cursor[i - 1..];
            let query = &prefix[1..].to_lowercase();

            let commands = [
                (r"\section{}", "Seção numerada principal", 1, CompletionKind::Command),
                (r"\subsection{}", "Subseção secundária", 1, CompletionKind::Command),
                (r"\subsubsection{}", "Sub-subseção detalhada", 1, CompletionKind::Command),
                (r"\paragraph{}", "Parágrafo com título em destaque", 1, CompletionKind::Command),
                (r"\textbf{}", "Texto em negrito", 1, CompletionKind::Command),
                (r"\textit{}", "Texto em itálico", 1, CompletionKind::Command),
                (r"\emph{}", "Ênfase tipográfica", 1, CompletionKind::Command),
                (r"\underline{}", "Texto sublinhado", 1, CompletionKind::Command),
                (r"\cite{}", "Citação bibliográfica", 1, CompletionKind::Citation),
                (r"\ref{}", "Referência cruzada a rótulo", 1, CompletionKind::Label),
                (r"\label{}", "Definição de âncora/rótulo", 1, CompletionKind::Label),
                (r"\begin{}", "Abertura de ambiente LaTeX", 1, CompletionKind::Environment),
                (r"\end{}", "Fechamento de ambiente", 1, CompletionKind::Environment),
                (r"\item ", "Item de lista", 0, CompletionKind::Command),
                (r"\usepackage{}", "Inclusão de pacote no preâmbulo", 1, CompletionKind::Command),
                (r"\documentclass{}", "Classe do documento (article, report...)", 1, CompletionKind::Command),
                (r"\title{}", "Título do documento", 1, CompletionKind::Command),
                (r"\author{}", "Autor da obra", 1, CompletionKind::Command),
                (r"\date{\today}", "Data atual ou customizada", 0, CompletionKind::Command),
                (r"\maketitle", "Gera o cabeçalho de título", 0, CompletionKind::Command),
                (r"\tableofcontents", "Gera o sumário automático", 0, CompletionKind::Command),
                (r"\frac{}{}", "Fração matemática", 3, CompletionKind::Math),
                (r"\sqrt{}", "Raiz quadrada", 1, CompletionKind::Math),
                (r"\sum_{}^{}", "Somatório com limites", 4, CompletionKind::Math),
                (r"\int_{}^{}", "Integral com limites", 4, CompletionKind::Math),
                (r"\infty", "Símbolo de infinito", 0, CompletionKind::Math),
                (r"\alpha", "Letra grega alfa", 0, CompletionKind::Math),
                (r"\beta", "Letra grega beta", 0, CompletionKind::Math),
                (r"\gamma", "Letra grega gama", 0, CompletionKind::Math),
                (r"\theta", "Letra grega teta", 0, CompletionKind::Math),
                (r"\lambda", "Letra grega lambda", 0, CompletionKind::Math),
                (r"\pi", "Constante pi", 0, CompletionKind::Math),
                (r"\sigma", "Letra grega sigma", 0, CompletionKind::Math),
                (r"\omega", "Letra grega ômega", 0, CompletionKind::Math),
                (r"\footnote{}", "Nota de rodapé", 1, CompletionKind::Command),
                (r"\caption{}", "Legenda de figura/tabela", 1, CompletionKind::Command),
                (r"\centering", "Centralização de conteúdo", 0, CompletionKind::Command),
                (r"\noindent", "Remove recuo de parágrafo", 0, CompletionKind::Command),
                (r"\vspace{}", "Espaçamento vertical", 1, CompletionKind::Command),
                (r"\hspace{}", "Espaçamento horizontal", 1, CompletionKind::Command),
                (r"\bibliography{}", "Arquivo .bib da bibliografia", 1, CompletionKind::Citation),
                (r"\bibliographystyle{}", "Estilo de citação (plain, abntex2...)", 1, CompletionKind::Citation),
            ];

            return commands
                .iter()
                .filter(|(cmd, _, _, _)| {
                    let cmd_name = &cmd[1..];
                    query.is_empty() || cmd_name.to_lowercase().starts_with(query)
                })
                .map(|(cmd, desc, rewind, kind)| CompletionItem {
                    label: cmd.to_string(),
                    insert_text: cmd.to_string(),
                    detail: desc.to_string(),
                    kind: *kind,
                    replace_len: prefix.len(),
                    cursor_rewind: *rewind,
                })
                .collect();
        }

        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_completion() {
        let items = CompletionEngine::query(r"\sec", &[], &[]);
        assert!(!items.is_empty());
        assert_eq!(items[0].label, r"\section{}");
        assert_eq!(items[0].replace_len, 4); // "\sec".len()
    }

    #[test]
    fn test_citation_completion() {
        let bibs = vec![BibSuggestion {
            cite_key: "einstein1905".to_string(),
            author: "Albert Einstein".to_string(),
            year: "1905".to_string(),
            title: "Elektrodynamik".to_string(),
        }];

        let items = CompletionEngine::query(r"Veja \cite{eins", &bibs, &[]);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label, "einstein1905");
        assert_eq!(items[0].insert_text, "einstein1905}");
        assert_eq!(items[0].replace_len, 4); // "eins".len()
    }

    #[test]
    fn test_environment_completion() {
        let items = CompletionEngine::query(r"\begin{equa", &[], &[]);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label, "equation");
        assert!(items[0].insert_text.contains(r"\end{equation}"));
    }

    #[test]
    fn test_no_trigger() {
        let items = CompletionEngine::query("Texto comum sem barra ", &[], &[]);
        assert!(items.is_empty());
    }
}
