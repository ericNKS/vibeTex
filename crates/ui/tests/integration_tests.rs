use vibetex_bibliography::BibIndex;
use vibetex_compiler::{CompilerRunner, LogParser, TexDetector};
use vibetex_core::{ChangeOrigin, ProjectConfig, Span, TextDelta};
use vibetex_document_model::SharedDocument;
use vibetex_latex_parser::*;
use vibetex_templates::TemplateCatalog;
use vibetex_workspace::ProjectManager;

#[test]
fn test_1_bidirectional_editing() {
    let source = r"\documentclass{article}
\begin{document}
\section{Título Inicial}
O texto contém \textbf{Original} aqui.
\end{document}";

    let mut doc = SharedDocument::new(source);
    let mut bold_inner_span = None;

    for node in &doc.ast().nodes {
        if let AstNode::Paragraph { inlines, .. } = node {
            for inline in inlines {
                if let InlineNode::Format { kind, inner_span, .. } = inline {
                    if *kind == FormatKind::Bold {
                        bold_inner_span = Some(*inner_span);
                    }
                }
            }
        }
    }

    let span = bold_inner_span.expect("Deve encontrar o span interno em negrito");
    assert_eq!(span.slice(doc.source()), "Original");

    // Edit visually to "Modificado"
    let delta = TextDelta::new(span, "Modificado", ChangeOrigin::VisualEditor);
    doc.apply_delta(delta).expect("Aplicação com sucesso");

    assert!(doc.source().contains(r"\textbf{Modificado}"));
    assert!(!doc.source().contains("Original"));
}

#[test]
fn test_2_preservation_of_commands_and_macros() {
    let source = r"\documentclass{article}
\newcommand{\customMacro}[1]{\emph{#1}}
\begin{document}
\section{Introdução}
Primeiro parágrafo com \cite{silva2026} e \medskip vertical.

\customMacro{Texto Personalizado}

Segundo parágrafo a ser modificado.
\end{document}";

    let mut doc = SharedDocument::new(source);

    // Modify only the second paragraph
    let target_str = "Segundo parágrafo a ser modificado.";
    let start = doc.source().find(target_str).unwrap();
    let span = Span::new(start, start + target_str.len());

    let delta = TextDelta::new(span, "Parágrafo completamente alterado com sucesso.", ChangeOrigin::VisualEditor);
    doc.apply_delta(delta).expect("Aplicação");

    // Verify all original macros, citations and spacing are 100% intact
    assert!(doc.source().contains(r"\newcommand{\customMacro}[1]{\emph{#1}}"));
    assert!(doc.source().contains(r"\cite{silva2026}"));
    assert!(doc.source().contains(r"\medskip"));
    assert!(doc.source().contains(r"\customMacro{Texto Personalizado}"));
    assert!(doc.source().contains("Parágrafo completamente alterado com sucesso."));
}

#[test]
fn test_3_file_preservation_on_open_and_save() {
    let source = "% Comentário no início\n\\documentclass{article}\n\\begin{document}\n  \\section{A}\n\\end{document}\n";
    let doc = SharedDocument::new(source);
    // Unedited source must be bit-for-bit identical
    assert_eq!(doc.source(), source);
}

#[test]
fn test_4_template_catalog_compatibility() {
    let temp_dir = tempfile::tempdir().unwrap();
    let templates = TemplateCatalog::all();

    assert!(templates.len() >= 6, "Pelo menos 6 templates base devem existir");

    for tmpl in templates {
        let sub_dir = temp_dir.path().join(tmpl.id);
        tmpl.instantiate(&sub_dir).expect("Instanciação do template");

        let main_tex = sub_dir.join("main.tex");
        assert!(main_tex.exists(), "Template {} deve gerar main.tex", tmpl.id);

        let content = std::fs::read_to_string(main_tex).unwrap();
        let ast = parse_latex(&content);
        assert!(ast.preamble.is_some() || !ast.nodes.is_empty(), "Template {} deve ser parseável", tmpl.id);
    }
}

#[test]
fn test_5_compiler_detection_and_log_parsing() {
    let report = TexDetector::detect_all();
    println!("LaTeXmk disponível: {}", report.latexmk.available);
    println!("pdfLaTeX disponível: {}", report.pdflatex.available);
    println!("Tectonic disponível: {}", report.tectonic.available);
    println!("Tectonic path: {:?}", report.tectonic.path);
    println!("Recomendado: {:?}", report.recommended_engine());

    let sample_log = r"! Undefined control sequence.
l.12 \erroDeSintaxe
Output written on main.pdf (2 pages).";

    let summary = LogParser::parse(sample_log);
    assert!(!summary.success);
    assert_eq!(summary.page_count, Some(2));
    assert_eq!(summary.diagnostics.len(), 1);
    assert_eq!(summary.diagnostics[0].line, Some(12));
}

#[test]
fn test_6_bibliography_management() {
    let bib = r"@article{knuth1984,
  author = {Donald E. Knuth},
  title = {Literate Programming},
  year = {1984}
}
@book{lamport1994,
  author = {Leslie Lamport},
  title = {LaTeX: A Document Preparation System},
  year = {1994}
}";

    let mut index = BibIndex::new();
    index.load_bib_file(bib);

    assert_eq!(index.all_entries().len(), 2);
    let knuth = index.get("knuth1984").expect("Encontrar knuth1984");
    assert_eq!(knuth.author(), "Donald E. Knuth");

    let search_res = index.search("programming");
    assert_eq!(search_res.len(), 1);
    assert_eq!(search_res[0].cite_key, "knuth1984");

    let missing = index.find_missing(&["knuth1984".into(), "inexistente2026".into()]);
    assert_eq!(missing, vec!["inexistente2026".to_string()]);
}

#[test]
fn test_8_unknown_commands_preserved_as_protected() {
    let source = r"\begin{document}
\meuComandoCustomizado{x}{y}{z}
\begin{diagramaEspecial}
bloco opaco
\end{diagramaEspecial}
\end{document}";

    let ast = parse_latex(source);
    let mut protected_count = 0;

    for node in &ast.nodes {
        match node {
            AstNode::ProtectedBlock { raw, .. } => {
                protected_count += 1;
                assert!(raw.contains("diagramaEspecial"));
            }
            AstNode::Paragraph { inlines, .. } => {
                for inline in inlines {
                    if let InlineNode::Protected { raw, .. } = inline {
                        protected_count += 1;
                        assert!(raw.contains("meuComandoCustomizado"));
                    }
                }
            }
            _ => {}
        }
    }

    assert!(protected_count >= 2, "Comandos desconhecidos devem ser protegidos e preservados");
}

#[test]
fn test_9_atomic_save_and_crash_recovery() {
    let temp_dir = tempfile::tempdir().unwrap();
    let file_path = temp_dir.path().join("main.tex");
    let content = "Conteúdo seguro em gravação atômica.";

    ProjectManager::atomic_write(&file_path, content.as_bytes()).expect("Gravação atômica");
    assert_eq!(std::fs::read_to_string(&file_path).unwrap(), content);

    ProjectManager::save_recovery(&file_path, "Conteúdo de recuperação pós-crash").expect("Recovery");
    let recovery_path = temp_dir.path().join(".main.tex.recovery~");
    assert!(recovery_path.exists());
    assert_eq!(std::fs::read_to_string(recovery_path).unwrap(), "Conteúdo de recuperação pós-crash");
}

#[test]
fn test_10_undo_redo_fidelity() {
    let initial = r"\section{Original} Texto";
    let mut doc = SharedDocument::new(initial);

    let span = Span::new(9, 17); // "Original"
    assert_eq!(span.slice(doc.source()), "Original");

    let delta = TextDelta::new(span, "Modificado", ChangeOrigin::VisualEditor);
    doc.apply_delta(delta).unwrap();
    assert_eq!(doc.source(), r"\section{Modificado} Texto");

    // Undo
    doc.undo().unwrap();
    assert_eq!(doc.source(), initial, "Undo deve restaurar o texto canônico bit-a-bit");

    // Redo
    doc.redo().unwrap();
    assert_eq!(doc.source(), r"\section{Modificado} Texto");
}

#[test]
fn test_11_real_compilation_and_pdf_generation() {
    let temp_dir = tempfile::tempdir().unwrap();
    let main_tex = temp_dir.path().join("main.tex");
    std::fs::write(&main_tex, r"\documentclass{article}\begin{document}Hello VibeTeX Real Engine!\end{document}").unwrap();
    let config = ProjectConfig::new("Test", "main.tex");
    let summary = CompilerRunner::compile(temp_dir.path(), &config, std::time::Duration::from_secs(30)).unwrap();
    assert!(summary.success, "Compilação deve ser bem-sucedida: {:?}", summary.raw_log);
    let pdf_path = CompilerRunner::get_pdf_path(temp_dir.path(), &config);
    assert!(pdf_path.exists(), "PDF deve ter sido gerado em {:?}", pdf_path);
    assert!(std::fs::metadata(&pdf_path).unwrap().len() > 0, "PDF não pode estar vazio");
}

