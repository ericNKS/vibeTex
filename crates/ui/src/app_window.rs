use adw::prelude::*;
use adw::{ActionRow, ApplicationWindow, HeaderBar, Toast, ToastOverlay, ViewStack, ViewSwitcher};
use gtk4::{Box, Button, Label, ListBox, Orientation, Paned, ScrolledWindow, Separator, Spinner, ToggleButton};
use std::cell::{Cell, RefCell};
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
    pdf_viewer: Rc<PdfViewerWidget>,
    state: Rc<RefCell<AppState>>,
    files_list: ListBox,
    outline_list: ListBox,
    bib_list: ListBox,
    status_label: Label,
    spinner: Spinner,
    log_content: Label,
    btn_auto_compile: ToggleButton,
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

        // 1. Left Header actions: Toggle Sidebar, New Project, Save, Undo, Redo
        let btn_toggle_sidebar = Button::builder()
            .icon_name("sidebar-show-symbolic")
            .tooltip_text("Mostrar / Ocultar Barra Lateral")
            .build();
        header.pack_start(&btn_toggle_sidebar);

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

        // 2. Center: Linked Mode Switcher [Código] [Ambos] [Visual]
        let mode_box = Box::new(Orientation::Horizontal, 0);
        mode_box.add_css_class("linked");

        let btn_mode_code = ToggleButton::builder()
            .label("Código")
            .icon_name("text-x-generic-symbolic")
            .tooltip_text("Somente Código LaTeX (Tela Cheia)")
            .build();

        let btn_mode_split = ToggleButton::builder()
            .label("Ambos")
            .icon_name("view-dual-symbolic")
            .tooltip_text("Código + PDF Lado a Lado (Estilo Overleaf)")
            .build();

        let btn_mode_visual = ToggleButton::builder()
            .label("Visual")
            .icon_name("view-preview-symbolic")
            .tooltip_text("Somente PDF Compilado (Tela Cheia)")
            .build();

        btn_mode_split.set_group(Some(&btn_mode_code));
        btn_mode_visual.set_group(Some(&btn_mode_code));
        btn_mode_split.set_active(true);

        mode_box.append(&btn_mode_code);
        mode_box.append(&btn_mode_split);
        mode_box.append(&btn_mode_visual);

        header.set_title_widget(Some(&mode_box));

        // 3. Right Header actions: Compile, Auto-Compile Switch, Spinner, Export
        let spinner = Spinner::new();
        spinner.set_margin_end(6);

        let btn_compile = Button::builder()
            .label("Compilar")
            .icon_name("media-playback-start-symbolic")
            .tooltip_text("Compilar PDF (Ctrl+B)")
            .css_classes(["suggested-action"])
            .build();

        let btn_auto_compile = ToggleButton::builder()
            .label("Auto")
            .active(true)
            .tooltip_text("Compilação automática contínua ao digitar")
            .build();

        header.pack_end(&btn_compile);
        header.pack_end(&btn_auto_compile);
        header.pack_end(&spinner);

        let btn_export = Button::from_icon_name("emblem-package-symbolic");
        btn_export.set_tooltip_text(Some("Exportar Projeto para ZIP"));
        header.pack_end(&btn_export);

        main_box.append(&header);

        // 4. Main Workspace Layout
        // root_paned: [Left Sidebar] <---> [Center/Right Workspace]
        let root_paned = Paned::new(Orientation::Horizontal);
        root_paned.set_vexpand(true);
        root_paned.set_hexpand(true);
        root_paned.set_position(260);
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

        // 5. Center & Right Workspace:
        // 5. Workspace Paned: [Left Area: Code + Logs] <---> [Right Area: PDF Viewer]
        let editor_pdf_paned = Paned::new(Orientation::Horizontal);
        editor_pdf_paned.set_vexpand(true);
        editor_pdf_paned.set_hexpand(true);
        editor_pdf_paned.set_position(570);

        let code_editor = Rc::new(CodeEditorWidget::new());
        code_editor.set_text_silently(&initial_text);

        // 6. Bottom Resizable & Minimizable Logs Drawer
        let bottom_drawer = Box::new(Orientation::Vertical, 0);
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

        let btn_minimize_drawer = Button::builder()
            .icon_name("pan-down-symbolic")
            .tooltip_text("Minimizar / Expandir Painel de Logs")
            .build();
        drawer_header.append(&btn_minimize_drawer);

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

        // Vertical Paned allowing the user to drag and adjust logs height
        let editor_logs_paned = Paned::new(Orientation::Vertical);
        editor_logs_paned.set_vexpand(true);
        editor_logs_paned.set_hexpand(true);
        editor_logs_paned.set_start_child(Some(code_editor.widget()));
        editor_logs_paned.set_end_child(Some(&bottom_drawer));
        editor_logs_paned.set_position(580);

        editor_pdf_paned.set_start_child(Some(&editor_logs_paned));

        // 7. PDF Viewer on the right
        let pdf_viewer = Rc::new(PdfViewerWidget::new());
        editor_pdf_paned.set_end_child(Some(pdf_viewer.widget()));

        root_paned.set_end_child(Some(&editor_pdf_paned));

        let app_win = Self {
            window,
            toast_overlay,
            code_editor,
            pdf_viewer,
            state,
            files_list,
            outline_list,
            bib_list,
            status_label,
            spinner,
            log_content,
            btn_auto_compile,
        };

        app_win.refresh_file_tree();
        app_win.refresh_outline();
        app_win.refresh_bib_list();

        let btn_compile_init = btn_compile.clone();

        app_win.setup_event_handlers(
            btn_save,
            btn_compile,
            btn_undo,
            btn_redo,
            btn_new_proj,
            btn_export,
            btn_clear_logs,
            btn_toggle_sidebar,
            btn_minimize_drawer,
            sidebar_box,
            root_paned,
            editor_logs_paned,
            editor_pdf_paned,
            log_scroll,
            btn_mode_code,
            btn_mode_visual,
            btn_mode_split,
        );

        // Check for pre-existing PDF or trigger automatic initial compilation
        let cfg = ProjectConfig::new("VibeTeX Project", "main.tex");
        let expected_pdf = CompilerRunner::get_pdf_path(&project_dir, &cfg);
        if expected_pdf.exists() {
            app_win.pdf_viewer.load_pdf(&expected_pdf);
        } else {
            // Automatically compile on startup so PDF appears immediately
            glib::idle_add_local_once(move || {
                btn_compile_init.emit_clicked();
            });
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

            row.connect_activated(move |_| {
                let cursor = code_clone.get_cursor_offset();
                let delta = DocumentOperations::insert_citation(cursor, &key_clone, ChangeOrigin::CodeEditor);
                let mut s = state_clone.borrow_mut();
                if let Ok(_) = s.document.apply_delta(delta) {
                    let updated = s.document.source().to_string();
                    code_clone.set_text_silently(&updated);
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
        btn_toggle_sidebar: Button,
        btn_minimize_drawer: Button,
        sidebar_box: Box,
        root_paned: Paned,
        editor_logs_paned: Paned,
        editor_pdf_paned: Paned,
        log_scroll: ScrolledWindow,
        btn_mode_code: ToggleButton,
        btn_mode_visual: ToggleButton,
        btn_mode_split: ToggleButton,
    ) {
        // --- 1. View Mode Switching: [Código] [Ambos] [Visual] ---
        let left_panel = editor_logs_paned.clone();
        let right_panel = self.pdf_viewer.widget().clone();
        let ws_paned = editor_pdf_paned.clone();

        btn_mode_code.connect_toggled({
            let left_p = left_panel.clone();
            let right_p = right_panel.clone();
            move |btn| {
                if btn.is_active() {
                    left_p.set_visible(true);
                    right_p.set_visible(false);
                }
            }
        });

        btn_mode_visual.connect_toggled({
            let left_p = left_panel.clone();
            let right_p = right_panel.clone();
            move |btn| {
                if btn.is_active() {
                    left_p.set_visible(false);
                    right_p.set_visible(true);
                }
            }
        });

        btn_mode_split.connect_toggled({
            let left_p = left_panel.clone();
            let right_p = right_panel.clone();
            let ws_p = ws_paned.clone();
            move |btn| {
                if btn.is_active() {
                    left_p.set_visible(true);
                    right_p.set_visible(true);
                    ws_p.set_position(570);
                }
            }
        });

        // --- 2. Collapsible Sidebar ---
        let sidebar_ref = sidebar_box.clone();
        let root_p_ref = root_paned.clone();
        btn_toggle_sidebar.connect_clicked(move |_| {
            let is_vis = sidebar_ref.is_visible();
            sidebar_ref.set_visible(!is_vis);
            if !is_vis {
                root_p_ref.set_position(260);
            }
        });

        // Automatically collapse sidebar if dragged very small (< 75px)
        let sidebar_auto_collapse = sidebar_box.clone();
        root_paned.connect_position_notify(move |paned| {
            if paned.position() < 75 && sidebar_auto_collapse.is_visible() {
                sidebar_auto_collapse.set_visible(false);
            }
        });

        // --- 3. Minimizable & Resizable Logs Drawer ---
        let is_drawer_minimized = Rc::new(Cell::new(false));
        let saved_drawer_pos = Rc::new(Cell::new(580));
        let log_scroll_ref = log_scroll.clone();
        let logs_paned_ref = editor_logs_paned.clone();
        let btn_min_ref = btn_minimize_drawer.clone();

        btn_minimize_drawer.connect_clicked(move |_| {
            let is_min = is_drawer_minimized.get();
            if is_min {
                // Restore expanded
                log_scroll_ref.set_visible(true);
                btn_min_ref.set_icon_name("pan-down-symbolic");
                btn_min_ref.set_tooltip_text(Some("Minimizar Painel de Logs"));
                logs_paned_ref.set_position(saved_drawer_pos.get());
                is_drawer_minimized.set(false);
            } else {
                // Minimize to status bar
                saved_drawer_pos.set(logs_paned_ref.position());
                log_scroll_ref.set_visible(false);
                btn_min_ref.set_icon_name("pan-up-symbolic");
                btn_min_ref.set_tooltip_text(Some("Expandir Painel de Logs"));
                is_drawer_minimized.set(true);
            }
        });

        // --- 4. Compilation Pipeline (Manual + Auto-compile with Debounce) ---
        let state_comp = self.state.clone();
        let pdf_comp = self.pdf_viewer.clone();
        let spinner_comp = self.spinner.clone();
        let status_comp = self.status_label.clone();
        let log_comp = self.log_content.clone();
        let toast_comp = self.toast_overlay.clone();

        let trigger_compile = Rc::new(move || {
            let (proj_dir, cur_file, content) = {
                let mut st = state_comp.borrow_mut();
                if st.is_compiling {
                    return;
                }
                st.is_compiling = true;
                (st.project_dir.clone(), st.current_file.clone(), st.document.source().to_string())
            };

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
                                let cfg = ProjectConfig::new("VibeTeX Project", "main.tex");
                                let pdf_path = CompilerRunner::get_pdf_path(&proj_dir_done, &cfg);
                                if pdf_path.exists() {
                                    pdf_done.load_pdf(&pdf_path);
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

        // Debounce timer with generation counter: zero risk of GLib SourceId panics
        let compile_generation: Rc<Cell<u64>> = Rc::new(Cell::new(0));
        let btn_auto_ref = self.btn_auto_compile.clone();
        let trigger_auto_compile = {
            let tc = trigger_compile.clone();
            let gen_counter = compile_generation.clone();
            let auto_btn = btn_auto_ref.clone();
            Rc::new(move || {
                if !auto_btn.is_active() {
                    return;
                }
                let current_gen = gen_counter.get().wrapping_add(1);
                gen_counter.set(current_gen);

                let tc_run = tc.clone();
                let gen_check = gen_counter.clone();
                glib::timeout_add_local_once(Duration::from_millis(400), move || {
                    if gen_check.get() == current_gen {
                        tc_run();
                    }
                });
            })
        };

        // Manual compile button
        let tc_manual = trigger_compile.clone();
        btn_compile.connect_clicked(move |_| {
            tc_manual();
        });

        // 5. Code Editor -> Document & Auto-Compile
        let state_code = self.state.clone();
        let code_ref = self.code_editor.clone();
        let auto_comp_code = trigger_auto_compile.clone();

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

            auto_comp_code();
        });

        // 7. Save Document
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

        // 8. Undo
        let state_undo = self.state.clone();
        let code_undo = self.code_editor.clone();
        let auto_comp_undo = trigger_auto_compile.clone();
        btn_undo.connect_clicked(move |_| {
            let mut st = state_undo.borrow_mut();
            if let Ok(Some(_)) = st.document.undo() {
                let text = st.document.source().to_string();
                code_undo.set_text_silently(&text);
                auto_comp_undo();
            }
        });

        // 9. Redo
        let state_redo = self.state.clone();
        let code_redo = self.code_editor.clone();
        let auto_comp_redo = trigger_auto_compile.clone();
        btn_redo.connect_clicked(move |_| {
            let mut st = state_redo.borrow_mut();
            if let Ok(Some(_)) = st.document.redo() {
                let text = st.document.source().to_string();
                code_redo.set_text_silently(&text);
                auto_comp_redo();
            }
        });

        // 10. Clear Logs
        let log_clear = self.log_content.clone();
        let status_clear = self.status_label.clone();
        btn_clear_logs.connect_clicked(move |_| {
            log_clear.set_text("");
            status_clear.set_text("Logs limpos.");
        });

        // 11. Export to ZIP (Background thread: non-blocking and safe)
        let state_zip = self.state.clone();
        let toast_zip = self.toast_overlay.clone();
        btn_export.connect_clicked(move |_| {
            let st = state_zip.borrow();
            let proj_dir = st.project_dir.clone();
            let zip_path = proj_dir.join("projeto_vibetex.zip");
            let toast_clone = toast_zip.clone();

            toast_clone.add_toast(Toast::new("Exportando projeto para ZIP em segundo plano..."));

            let (sender, receiver) = async_channel::bounded(1);
            let zip_target = zip_path.clone();

            std::thread::spawn(move || {
                let res = ZipService::export_project(&proj_dir, &zip_target);
                let _ = sender.send_blocking(res);
            });

            glib::spawn_future_local(async move {
                if let Ok(res) = receiver.recv().await {
                    match res {
                        Ok(_) => {
                            toast_clone.add_toast(Toast::new(&format!("Projeto exportado: {}", zip_path.display())));
                        }
                        Err(e) => {
                            toast_clone.add_toast(Toast::new(&format!("Erro ao exportar ZIP: {}", e)));
                        }
                    }
                }
            });
        });

        // 12. New Project from Template Dialog
        let toast_tmpl = self.toast_overlay.clone();
        let state_tmpl = self.state.clone();
        let code_tmpl = self.code_editor.clone();
        let tc_tmpl = trigger_compile.clone();

        btn_new_proj.connect_clicked(move |_| {
            if let Some(template) = TemplateCatalog::get("abnt") {
                let st = state_tmpl.borrow();
                let _ = template.instantiate(&st.project_dir);
                let main_path = st.project_dir.join("main.tex");
                if let Ok(content) = std::fs::read_to_string(&main_path) {
                    code_tmpl.set_text_silently(&content);
                    toast_tmpl.add_toast(Toast::new("Modelo ABNT aplicado com sucesso!"));
                    tc_tmpl();
                }
            }
        });
    }
}
