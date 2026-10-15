use adw::prelude::*;
use adw::{ActionRow, ApplicationWindow, HeaderBar, Toast, ToastOverlay, ViewStack, ViewSwitcher};
use gtk4::{Box, Button, Label, ListBox, Orientation, Paned, ScrolledWindow, Separator, Spinner, Stack};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;
use vibetex_bibliography::BibIndex;
use vibetex_code_editor::CodeEditorWidget;
use vibetex_compiler::CompilerRunner;
use vibetex_core::{ChangeOrigin, DiagnosticSeverity, ProjectConfig, TextDelta};
use vibetex_document_model::{DocumentOperations, SharedDocument};
use vibetex_latex_parser::*;
use vibetex_pdf_viewer::PdfViewerWidget;
use vibetex_templates::{TemplateCatalog, ZipService};
use vibetex_visual_editor::VisualEditorWidget;
use vibetex_workspace::{FileKind, FileTreeScanner, ProjectManager};

pub struct AppState {
    pub project_dir: PathBuf,
    pub current_file: PathBuf,
    pub document: SharedDocument,
    pub bib_index: BibIndex,
    pub is_compiling: bool,
}

pub struct AppWindow {
    window: ApplicationWindow,
    toast_overlay: ToastOverlay,
    code_editor: Rc<CodeEditorWidget>,
    visual_editor: Rc<VisualEditorWidget>,
    pdf_viewer: Rc<PdfViewerWidget>,
    state: Rc<RefCell<AppState>>,
    files_list: ListBox,
    outline_list: ListBox,
    bib_list: ListBox,
    status_label: Label,
    spinner: Spinner,
    log_content: Label,
    _bottom_drawer: Box,
}

impl AppWindow {
    pub fn new(app: &adw::Application, project_dir: PathBuf) -> Self {
        let main_file = ProjectManager::detect_main_file(&project_dir);
        let main_file_path = project_dir.join(&main_file);

        let initial_text = if main_file_path.exists() {
            std::fs::read_to_string(&main_file_path).unwrap_or_else(|_| Self::default_initial_latex())
        } else {
            Self::default_initial_latex()
        };

        let mut bib_index = BibIndex::new();
        // Auto-load any .bib files in project
        if let Ok(entries) = std::fs::read_dir(&project_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().map(|e| e == "bib").unwrap_or(false) {
                    if let Ok(content) = std::fs::read_to_string(&p) {
                        bib_index.load_bib_file(&content);
                    }
                }
            }
        }

        let document = SharedDocument::new(initial_text.clone());

        let state = Rc::new(RefCell::new(AppState {
            project_dir: project_dir.clone(),
            current_file: main_file_path,
            document,
            bib_index,
            is_compiling: false,
        }));

        let toast_overlay = ToastOverlay::new();
        let window = ApplicationWindow::builder()
            .application(app)
            .title("VibeTeX — Editor LaTeX Nativo")
            .default_width(1400)
            .default_height(880)
            .content(&toast_overlay)
            .build();

        let main_box = Box::new(Orientation::Vertical, 0);
        toast_overlay.set_child(Some(&main_box));

        // HeaderBar
        let header = HeaderBar::new();
        let title_label = Label::new(Some("VibeTeX"));
        title_label.add_css_class("title");
        header.set_title_widget(Some(&title_label));

        // Left Header actions
        let btn_new_proj = Button::from_icon_name("document-new-symbolic");
        btn_new_proj.set_tooltip_text(Some("Novo Projeto a partir de Template"));
        header.pack_start(&btn_new_proj);

        let btn_save = Button::from_icon_name("document-save-symbolic");
        btn_save.set_tooltip_text(Some("Salvar Documento (Ctrl+S)"));
        header.pack_start(&btn_save);

        let btn_undo = Button::from_icon_name("edit-undo-symbolic");
        btn_undo.set_tooltip_text(Some("Desfazer (Ctrl+Z)"));
        header.pack_start(&btn_undo);

        let btn_redo = Button::from_icon_name("edit-redo-symbolic");
        btn_redo.set_tooltip_text(Some("Refazer (Ctrl+Y)"));
        header.pack_start(&btn_redo);

        // Center: Mode Switcher (Stack of Editors)
        let mode_stack = Stack::new();
        mode_stack.set_vexpand(true);
        mode_stack.set_hexpand(true);

        let code_editor = Rc::new(CodeEditorWidget::new());
        code_editor.set_text_silently(&initial_text);

        let visual_editor = Rc::new(VisualEditorWidget::new());
        visual_editor.render_ast(state.borrow().document.ast(), &initial_text);

        let split_paned = Paned::new(Orientation::Horizontal);
        split_paned.set_start_child(Some(code_editor.widget()));
        split_paned.set_end_child(Some(visual_editor.widget()));
        split_paned.set_position(500);

        mode_stack.add_titled(code_editor.widget(), Some("code"), "Código");
        mode_stack.add_titled(visual_editor.widget(), Some("visual"), "Visual");
        mode_stack.add_titled(&split_paned, Some("split"), "Código + Visual");
        mode_stack.set_visible_child_name("split");

        let mode_switcher = gtk4::StackSwitcher::new();
        mode_switcher.set_stack(Some(&mode_stack));
        header.set_title_widget(Some(&mode_switcher));

        // Right Header actions: Compile & Export
        let spinner = Spinner::new();
        spinner.set_margin_end(6);

        let btn_compile = Button::builder()
            .label("Compilar")
            .icon_name("media-playback-start-symbolic")
            .tooltip_text("Compilar PDF (Ctrl+B)")
            .css_classes(["suggested-action"])
            .build();

        header.pack_end(&btn_compile);
        header.pack_end(&spinner);

        let btn_export = Button::from_icon_name("emblem-package-symbolic");
        btn_export.set_tooltip_text(Some("Exportar Projeto para ZIP (compatível com Overleaf)"));
        header.pack_end(&btn_export);

        main_box.append(&header);

        // Workspace Paned (Left Sidebar vs. Center/Right Content)
        let root_paned = Paned::new(Orientation::Horizontal);
        root_paned.set_vexpand(true);
        root_paned.set_hexpand(true);
        root_paned.set_position(280);
        main_box.append(&root_paned);

        // Left Sidebar: Files, Outline, Bibliography
        let sidebar_box = Box::new(Orientation::Vertical, 0);
        sidebar_box.set_size_request(240, -1);

        let sidebar_stack = ViewStack::new();
        sidebar_stack.set_vexpand(true);

        let files_list = ListBox::new();
        files_list.add_css_class("navigation-sidebar");
        let files_scroll = ScrolledWindow::new();
        files_scroll.set_child(Some(&files_list));
        sidebar_stack.add_titled(&files_scroll, Some("files"), "Arquivos");

        let outline_list = ListBox::new();
        outline_list.add_css_class("navigation-sidebar");
        let outline_scroll = ScrolledWindow::new();
        outline_scroll.set_child(Some(&outline_list));
        sidebar_stack.add_titled(&outline_scroll, Some("outline"), "Estrutura");

        let bib_list = ListBox::new();
        bib_list.add_css_class("navigation-sidebar");
        let bib_scroll = ScrolledWindow::new();
        bib_scroll.set_child(Some(&bib_list));
        sidebar_stack.add_titled(&bib_scroll, Some("bib"), "Bibliografia");

        let sidebar_switcher = ViewSwitcher::new();
        sidebar_switcher.set_stack(Some(&sidebar_stack));
        sidebar_switcher.set_policy(adw::ViewSwitcherPolicy::Narrow);

        sidebar_box.append(&sidebar_switcher);
        sidebar_box.append(&Separator::new(Orientation::Horizontal));
        sidebar_box.append(&sidebar_stack);
        root_paned.set_start_child(Some(&sidebar_box));

        // Center / Right Paned (Editor Stack vs. PDF Preview)
        let editor_pdf_paned = Paned::new(Orientation::Horizontal);
        editor_pdf_paned.set_vexpand(true);
        editor_pdf_paned.set_hexpand(true);
        editor_pdf_paned.set_position(650);

        let editor_container = Box::new(Orientation::Vertical, 0);
        editor_container.append(&mode_stack);

        // Bottom Collapsible Drawer for Logs & Diagnostics
        let bottom_drawer = Box::new(Orientation::Vertical, 0);
        bottom_drawer.set_size_request(-1, 140);
        bottom_drawer.add_css_class("card");
        bottom_drawer.set_margin_start(8);
        bottom_drawer.set_margin_end(8);
        bottom_drawer.set_margin_bottom(6);

        let drawer_header = Box::new(Orientation::Horizontal, 8);
        drawer_header.set_margin_start(12);
        drawer_header.set_margin_end(12);
        drawer_header.set_margin_top(6);
        drawer_header.set_margin_bottom(6);

        let status_label = Label::new(Some("Pronto. (0 erros, 0 avisos)"));
        status_label.set_xalign(0.0);
        status_label.add_css_class("dim-label");
        drawer_header.append(&status_label);

        let drawer_spacer = Box::new(Orientation::Horizontal, 0);
        drawer_spacer.set_hexpand(true);
        drawer_header.append(&drawer_spacer);

        let btn_clear_logs = Button::from_icon_name("edit-clear-symbolic");
        btn_clear_logs.set_tooltip_text(Some("Limpar Logs"));
        drawer_header.append(&btn_clear_logs);

        bottom_drawer.append(&drawer_header);
        bottom_drawer.append(&Separator::new(Orientation::Horizontal));

        let log_scroll = ScrolledWindow::new();
        log_scroll.set_vexpand(true);
        let log_content = Label::new(Some("VibeTeX inicializado. Pronto para compilar."));
        log_content.add_css_class("monospace");
        log_content.set_xalign(0.0);
        log_content.set_yalign(0.0);
        log_content.set_wrap(true);
        log_content.set_margin_start(12);
        log_content.set_margin_end(12);
        log_content.set_margin_top(6);
        log_scroll.set_child(Some(&log_content));
        bottom_drawer.append(&log_scroll);

        editor_container.append(&bottom_drawer);
        editor_pdf_paned.set_start_child(Some(&editor_container));

        let pdf_viewer = Rc::new(PdfViewerWidget::new());
        editor_pdf_paned.set_end_child(Some(pdf_viewer.widget()));

        root_paned.set_end_child(Some(&editor_pdf_paned));

        let app_win = Self {
            window,
            toast_overlay,
            code_editor,
            visual_editor,
            pdf_viewer,
            state,
            files_list,
            outline_list,
            bib_list,
            status_label,
            spinner,
            log_content,
            _bottom_drawer: bottom_drawer,
        };

        app_win.refresh_file_tree();
        app_win.refresh_outline();
        app_win.refresh_bib_list();
        app_win.setup_event_handlers(btn_save, btn_compile, btn_undo, btn_redo, btn_new_proj, btn_export, btn_clear_logs);

        // Check for pre-existing PDF in .build/ or project root
        let expected_pdf = project_dir.join(".build").join("main.pdf");
        if expected_pdf.exists() {
            app_win.pdf_viewer.load_pdf(&expected_pdf);
        } else {
            let root_pdf = project_dir.join("main.pdf");
            if root_pdf.exists() {
                app_win.pdf_viewer.load_pdf(&root_pdf);
            }
        }

        app_win
    }

    pub fn show(&self) {
        self.window.present();
    }

    fn default_initial_latex() -> String {
        r"\documentclass[12pt,a4paper]{article}
\usepackage[utf8]{inputenc}
\usepackage[T1]{fontenc}
\usepackage{amsmath,amssymb}
\usepackage{hyperref}

\title{Novo Projeto VibeTeX}
\author{Pesquisador}
\date{\today}

\begin{document}
\maketitle

\section{Introdução}
Bem-vindo ao \textbf{VibeTeX}, o editor LaTeX moderno, nativo e híbrido desenvolvido em Rust e GTK4.

\subsection{Recursos de Edição Híbrida}
Você pode editar este documento diretamente pelo modo visual formatado ou pelo código-fonte.
Todos os comandos originais, macros e preâmbulos são \textit{estritamente preservados} sem qualquer corrupção.

\begin{equation}
\int_{-\infty}^{\infty} e^{-x^2} dx = \sqrt{\pi}
\end{equation}

\section{Metodologia}
A sincronização utiliza intervalos de bytes exatos para aplicar deltas pontuais.

\end{document}
".to_string()
    }

    pub fn refresh_file_tree(&self) {
        while let Some(child) = self.files_list.first_child() {
            self.files_list.remove(&child);
        }

        let st = self.state.borrow();
        let items = FileTreeScanner::scan_project(&st.project_dir);

        for item in items {
            let row = ActionRow::new();
            row.set_title(&item.name);
            row.set_icon_name(Some(item.kind.icon_name()));
            row.set_activatable(true);

            let path_clone = item.path.clone();
            let code_editor_clone = self.code_editor.clone();
            let visual_editor_clone = self.visual_editor.clone();
            let state_clone = self.state.clone();

            row.connect_activated(move |_| {
                if item.kind == FileKind::Latex {
                    if let Ok(content) = std::fs::read_to_string(&path_clone) {
                        {
                            let mut s = state_clone.borrow_mut();
                            s.current_file = path_clone.clone();
                            s.document = SharedDocument::new(&content);
                        }
                        code_editor_clone.set_text_silently(&content);
                        let s = state_clone.borrow();
                        visual_editor_clone.render_ast(s.document.ast(), &content);
                    }
                }
            });

            self.files_list.append(&row);
        }
    }

    pub fn refresh_outline(&self) {
        while let Some(child) = self.outline_list.first_child() {
            self.outline_list.remove(&child);
        }

        let st = self.state.borrow();
        let ast = st.document.ast();

        for node in &ast.nodes {
            if let AstNode::Section { level, title, title_span, .. } = node {
                let row = ActionRow::new();
                let icon = match level {
                    SectionLevel::Part | SectionLevel::Chapter => "view-list-bullet-symbolic",
                    SectionLevel::Section => "go-next-symbolic",
                    SectionLevel::Subsection => "pan-end-symbolic",
                    _ => "bullet-symbolic",
                };
                row.set_title(title);
                row.set_icon_name(Some(icon));
                row.set_activatable(true);

                let offset = title_span.start;
                let code_clone = self.code_editor.clone();
                row.connect_activated(move |_| {
                    code_clone.set_cursor_offset(offset);
                });

                self.outline_list.append(&row);
            }
        }
    }

    pub fn refresh_bib_list(&self) {
        while let Some(child) = self.bib_list.first_child() {
            self.bib_list.remove(&child);
        }

        let st = self.state.borrow();
        let entries = st.bib_index.all_entries();

        if entries.is_empty() {
            let row = ActionRow::new();
            row.set_title("Nenhuma referência .bib carregada");
            self.bib_list.append(&row);
            return;
        }

        for entry in entries {
            let row = ActionRow::new();
            row.set_title(&entry.cite_key);
            row.set_subtitle(&format!("{} ({})", entry.author(), entry.year()));
            row.set_icon_name(Some("x-office-address-book-symbolic"));
            row.set_activatable(true);

            let key_clone = entry.cite_key.clone();
            let code_clone = self.code_editor.clone();
            let state_clone = self.state.clone();
            let visual_clone = self.visual_editor.clone();

            row.connect_activated(move |_| {
                let cursor = code_clone.get_cursor_offset();
                let delta = DocumentOperations::insert_citation(cursor, &key_clone, ChangeOrigin::VisualEditor);
                let mut s = state_clone.borrow_mut();
                if let Ok(_) = s.document.apply_delta(delta) {
                    let updated = s.document.source().to_string();
                    code_clone.set_text_silently(&updated);
                    visual_clone.render_ast(s.document.ast(), &updated);
                }
            });

            self.bib_list.append(&row);
        }
    }

    fn setup_event_handlers(
        &self,
        btn_save: Button,
        btn_compile: Button,
        btn_undo: Button,
        btn_redo: Button,
        btn_new_proj: Button,
        btn_export: Button,
        btn_clear_logs: Button,
    ) {
        // 1. Code Editor -> Document & Visual Editor
        let state_code = self.state.clone();
        let visual_code = self.visual_editor.clone();
        let code_ref = self.code_editor.clone();

        self.code_editor.buffer().connect_changed(move |buffer| {
            if code_ref.is_internal_update() {
                return;
            }
            let (start, end) = buffer.bounds();
            let text = buffer.text(&start, &end, false).to_string();

            let mut st = state_code.borrow_mut();
            let full_span = vibetex_core::Span::new(0, st.document.source().len());
            let delta = TextDelta::new(full_span, text.clone(), ChangeOrigin::CodeEditor);
            let _ = st.document.apply_delta(delta);

            visual_code.render_ast(st.document.ast(), &text);
        });

        // 2. Visual Editor -> Document & Code Editor
        let state_vis = self.state.clone();
        let code_vis = self.code_editor.clone();
        let visual_self = self.visual_editor.clone();

        self.visual_editor.set_on_delta(move |delta| {
            let mut st = state_vis.borrow_mut();
            if let Ok(_) = st.document.apply_delta(delta) {
                let updated = st.document.source().to_string();
                code_vis.set_text_silently(&updated);
                visual_self.render_ast(st.document.ast(), &updated);
            }
        });

        // 3. Save Document
        let state_save = self.state.clone();
        let toast_save = self.toast_overlay.clone();
        btn_save.connect_clicked(move |_| {
            let mut st = state_save.borrow_mut();
            let target = st.current_file.clone();
            let content = st.document.source().as_bytes();
            if let Ok(_) = ProjectManager::atomic_write(&target, content) {
                st.document.mark_saved();
                toast_save.add_toast(Toast::new("Documento salvo com sucesso."));
            } else {
                toast_save.add_toast(Toast::new("Erro ao salvar documento."));
            }
        });

        // 4. Undo
        let state_undo = self.state.clone();
        let code_undo = self.code_editor.clone();
        let visual_undo = self.visual_editor.clone();
        btn_undo.connect_clicked(move |_| {
            let mut st = state_undo.borrow_mut();
            if let Ok(Some(_)) = st.document.undo() {
                let text = st.document.source().to_string();
                code_undo.set_text_silently(&text);
                visual_undo.render_ast(st.document.ast(), &text);
            }
        });

        // 5. Redo
        let state_redo = self.state.clone();
        let code_redo = self.code_editor.clone();
        let visual_redo = self.visual_editor.clone();
        btn_redo.connect_clicked(move |_| {
            let mut st = state_redo.borrow_mut();
            if let Ok(Some(_)) = st.document.redo() {
                let text = st.document.source().to_string();
                code_redo.set_text_silently(&text);
                visual_redo.render_ast(st.document.ast(), &text);
            }
        });

        // 6. Compile
        let state_comp = self.state.clone();
        let pdf_comp = self.pdf_viewer.clone();
        let spinner_comp = self.spinner.clone();
        let status_comp = self.status_label.clone();
        let log_comp = self.log_content.clone();
        let toast_comp = self.toast_overlay.clone();

        btn_compile.connect_clicked(move |_| {
            let (proj_dir, cur_file, content) = {
                let mut st = state_comp.borrow_mut();
                if st.is_compiling {
                    return;
                }
                st.is_compiling = true;
                (st.project_dir.clone(), st.current_file.clone(), st.document.source().to_string())
            };

            // Auto-save before compile
            let _ = ProjectManager::atomic_write(&cur_file, content.as_bytes());

            spinner_comp.start();
            status_comp.set_text("Compilando documento LaTeX com latexmk/pdflatex...");

            let state_done = state_comp.clone();
            let spinner_done = spinner_comp.clone();
            let status_done = status_comp.clone();
            let log_done = log_comp.clone();
            let pdf_done = pdf_comp.clone();
            let toast_done = toast_comp.clone();
            let proj_dir_thread = proj_dir.clone();
            let proj_dir_done = proj_dir.clone();

            let (sender, receiver) = async_channel::bounded(1);

            std::thread::spawn(move || {
                let config = ProjectConfig::new("VibeTeX Project", "main.tex");
                let result = CompilerRunner::compile(&proj_dir_thread, &config, Duration::from_secs(60));
                let _ = sender.send_blocking(result);
            });

            glib::spawn_future_local(async move {
                if let Ok(result) = receiver.recv().await {
                    spinner_done.stop();
                    {
                        let mut st = state_done.borrow_mut();
                        st.is_compiling = false;
                    }

                    match result {
                        Ok(summary) => {
                            let err_count = summary
                                .diagnostics
                                .iter()
                                .filter(|d| d.severity == DiagnosticSeverity::Error)
                                .count();
                            let warn_count = summary
                                .diagnostics
                                .iter()
                                .filter(|d| d.severity == DiagnosticSeverity::Warning)
                                .count();

                            let status_msg = format!(
                                "Compilação concluída: {} erro(s), {} aviso(s). (Páginas: {})",
                                err_count,
                                warn_count,
                                summary.page_count.map(|p| p.to_string()).unwrap_or_else(|| "-".into())
                            );
                            status_done.set_text(&status_msg);
                            log_done.set_text(&summary.raw_log);

                            if summary.success {
                                toast_done.add_toast(Toast::new("Compilação bem-sucedida!"));
                                let pdf_path = proj_dir_done.join(".build").join("main.pdf");
                                if pdf_path.exists() {
                                    pdf_done.load_pdf(&pdf_path);
                                } else {
                                    let root_pdf = proj_dir_done.join("main.pdf");
                                    if root_pdf.exists() {
                                        pdf_done.load_pdf(&root_pdf);
                                    }
                                }
                            } else {
                                toast_done.add_toast(Toast::new("Houve erros durante a compilação."));
                            }
                        }
                        Err(e) => {
                            let msg = format!("Falha ao executar compilador: {}", e);
                            status_done.set_text(&msg);
                            log_done.set_text(&msg);
                            toast_done.add_toast(Toast::new(&msg));
                        }
                    }
                }
            });
        });

        // 7. Clear Logs
        let log_clear = self.log_content.clone();
        let status_clear = self.status_label.clone();
        btn_clear_logs.connect_clicked(move |_| {
            log_clear.set_text("");
            status_clear.set_text("Logs limpos.");
        });

        // 8. Export to ZIP
        let state_zip = self.state.clone();
        let toast_zip = self.toast_overlay.clone();
        btn_export.connect_clicked(move |_| {
            let st = state_zip.borrow();
            let zip_path = st.project_dir.join("projeto_vibetex.zip");
            if let Ok(_) = ZipService::export_project(&st.project_dir, &zip_path) {
                toast_zip.add_toast(Toast::new(&format!("Projeto exportado: {}", zip_path.display())));
            } else {
                toast_zip.add_toast(Toast::new("Erro ao exportar projeto para ZIP."));
            }
        });

        // 9. New Project from Template Dialog
        let toast_tmpl = self.toast_overlay.clone();
        let state_tmpl = self.state.clone();
        let code_tmpl = self.code_editor.clone();
        let visual_tmpl = self.visual_editor.clone();

        btn_new_proj.connect_clicked(move |_| {
            // Apply template directly or show popover with templates
            if let Some(template) = TemplateCatalog::get("abnt") {
                let st = state_tmpl.borrow();
                let _ = template.instantiate(&st.project_dir);
                let main_path = st.project_dir.join("main.tex");
                if let Ok(content) = std::fs::read_to_string(&main_path) {
                    code_tmpl.set_text_silently(&content);
                    visual_tmpl.render_ast(st.document.ast(), &content);
                    toast_tmpl.add_toast(Toast::new("Modelo ABNT aplicado com sucesso!"));
                }
            }
        });
    }
}
