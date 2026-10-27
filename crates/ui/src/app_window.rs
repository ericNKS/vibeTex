use adw::prelude::*;
use adw::{ActionRow, ApplicationWindow, HeaderBar, Toast, ToastOverlay, ViewStack, ViewSwitcher};
use gtk4::gdk::Key;
use gtk4::{
    Box, Button, Entry, EventControllerKey, FileChooserAction, FileChooserNative, FileFilter, GestureClick, Image, Label,
    ListBox, ListBoxRow, Orientation, Paned, Popover, ResponseType, ScrolledWindow, Separator, Spinner, TextView,
    ToggleButton, WrapMode,
};
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;
use vibetex_bibliography::BibIndex;
use vibetex_code_editor::{BibSuggestion, CodeEditorWidget, LanguageMode};
use vibetex_compiler::CompilerRunner;
use vibetex_core::{ChangeOrigin, DiagnosticSeverity, ProjectConfig, TextDelta};
use vibetex_document_model::{DocumentOperations, SharedDocument};
use vibetex_latex_parser::*;
use vibetex_pdf_viewer::PdfViewerWidget;
use vibetex_templates::{TemplateCatalog, ZipService};
use vibetex_workspace::{
    FileItem, FileKind, FileTreeScanner, ProjectManager, ProjectSession, SessionManager,
};

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
    log_view: TextView,
    btn_auto_compile: ToggleButton,
    #[allow(dead_code)]
    btn_toggle_logs: ToggleButton,
    #[allow(dead_code)]
    btn_compile: Button,
    collapsed_dirs: Rc<RefCell<HashSet<PathBuf>>>,
}

impl AppWindow {
    pub fn new(app: &adw::Application, project_dir: PathBuf) -> Self {
        let mut global_session = SessionManager::load();
        global_session.last_project_dir = Some(project_dir.clone());
        let _ = SessionManager::save(&global_session);

        let proj_session = SessionManager::load_project_session(&project_dir, &global_session);

        let current_file_path = if let Some(ref saved_file) = proj_session.current_file {
            if saved_file.exists() && saved_file.is_file() {
                saved_file.clone()
            } else {
                let main_file = ProjectManager::detect_main_file(&project_dir);
                project_dir.join(&main_file)
            }
        } else {
            let main_file = ProjectManager::detect_main_file(&project_dir);
            project_dir.join(&main_file)
        };

        let initial_text = if current_file_path.exists() {
            std::fs::read_to_string(&current_file_path).unwrap_or_else(|_| Self::default_initial_latex())
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
            current_file: current_file_path,
            document,
            bib_index,
            is_compiling: false,
        }));

        let toast_overlay = ToastOverlay::new();

        let default_w = if global_session.window_width > 400 {
            global_session.window_width
        } else {
            1400
        };
        let default_h = if global_session.window_height > 300 {
            global_session.window_height
        } else {
            880
        };

        let window = ApplicationWindow::builder()
            .application(app)
            .title("VibeTeX — Editor LaTeX Nativo")
            .default_width(default_w)
            .default_height(default_h)
            .content(&toast_overlay)
            .build();

        let main_box = Box::new(Orientation::Vertical, 0);
        toast_overlay.set_child(Some(&main_box));

        // HeaderBar
        let header = HeaderBar::new();

        // 1. Left Header actions: Toggle Sidebar, Open Project, New Project, Save, Undo, Redo
        let btn_toggle_sidebar = Button::builder()
            .icon_name("sidebar-show-symbolic")
            .tooltip_text("Mostrar / Ocultar Barra Lateral")
            .build();
        header.pack_start(&btn_toggle_sidebar);

        let btn_open_proj = Button::builder()
            .icon_name("document-open-symbolic")
            .tooltip_text("Abrir Pasta de Projeto LaTeX (Ctrl+O)")
            .build();
        header.pack_start(&btn_open_proj);

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

        let saved_view_mode = global_session.view_mode.as_deref().unwrap_or("split");
        match saved_view_mode {
            "code" => {
                btn_mode_code.set_active(true);
            }
            "visual" => {
                btn_mode_visual.set_active(true);
            }
            _ => {
                btn_mode_split.set_active(true);
            }
        }

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

        let btn_export_menu = Button::builder()
            .label("Exportar")
            .icon_name("document-save-symbolic")
            .tooltip_text("Exportar PDF ou Projeto LaTeX (.zip)")
            .build();

        let pop_export = Popover::new();
        pop_export.set_parent(&btn_export_menu);

        let pop_export_box = Box::new(Orientation::Vertical, 4);
        pop_export_box.set_margin_start(8);
        pop_export_box.set_margin_end(8);
        pop_export_box.set_margin_top(8);
        pop_export_box.set_margin_bottom(8);

        let btn_export_pdf = Button::builder()
            .label("Exportar PDF (.pdf)")
            .icon_name("document-save-symbolic")
            .tooltip_text("Salvar arquivo PDF compilado (Ctrl+Shift+S)")
            .css_classes(["flat"])
            .build();

        let btn_export_zip = Button::builder()
            .label("Exportar Projeto (.zip)")
            .icon_name("folder-download-symbolic")
            .tooltip_text("Salvar cópia compactada do projeto (Ctrl+Shift+E)")
            .css_classes(["flat"])
            .build();

        pop_export_box.append(&btn_export_pdf);
        pop_export_box.append(&Separator::new(Orientation::Horizontal));
        pop_export_box.append(&btn_export_zip);
        pop_export.set_child(Some(&pop_export_box));

        {
            let pop_exp_c = pop_export.clone();
            btn_export_menu.connect_clicked(move |_| {
                pop_exp_c.popup();
            });
        }

        header.pack_end(&btn_export_menu);

        main_box.append(&header);

        // 4. Main Workspace Layout
        // root_paned: [Left Sidebar] <---> [Center/Right Workspace]
        let root_paned = Paned::new(Orientation::Horizontal);
        root_paned.set_vexpand(true);
        root_paned.set_hexpand(true);
        root_paned.set_position(260);
        main_box.append(&root_paned);

        let code_editor = Rc::new(CodeEditorWidget::new());
        code_editor.set_text_silently(&initial_text);
        let initial_collapsed: HashSet<PathBuf> = proj_session.collapsed_dirs.into_iter().collect();
        let collapsed_dirs = Rc::new(RefCell::new(initial_collapsed));

        // Left Sidebar: Files, Outline, Bibliography
        let sidebar_box = Box::new(Orientation::Vertical, 0);
        sidebar_box.set_size_request(240, -1);

        let sidebar_stack = ViewStack::new();
        sidebar_stack.set_vexpand(true);

        // 1. Files Tab: Toolbar with Add File / Add Folder / Refresh, plus Tree List
        let files_tab_box = Box::new(Orientation::Vertical, 0);

        let files_toolbar = Box::new(Orientation::Horizontal, 4);
        files_toolbar.set_margin_start(8);
        files_toolbar.set_margin_end(8);
        files_toolbar.set_margin_top(6);
        files_toolbar.set_margin_bottom(6);

        let files_tab_title = Label::new(Some("ARQUIVOS"));
        files_tab_title.add_css_class("caption-heading");
        files_tab_title.set_xalign(0.0);
        files_toolbar.append(&files_tab_title);

        let files_spacer = Box::new(Orientation::Horizontal, 0);
        files_spacer.set_hexpand(true);
        files_toolbar.append(&files_spacer);

        let btn_add_file = Button::builder()
            .icon_name("document-new-symbolic")
            .tooltip_text("Novo Arquivo...")
            .css_classes(["flat", "circular"])
            .build();
        files_toolbar.append(&btn_add_file);

        let btn_add_folder = Button::builder()
            .icon_name("folder-new-symbolic")
            .tooltip_text("Nova Pasta...")
            .css_classes(["flat", "circular"])
            .build();
        files_toolbar.append(&btn_add_folder);

        let btn_collapse_all = Button::builder()
            .icon_name("folder-symbolic")
            .tooltip_text("Minimizar Tudo (Recolher todas as pastas)")
            .css_classes(["flat", "circular"])
            .build();
        files_toolbar.append(&btn_collapse_all);

        let btn_expand_all = Button::builder()
            .icon_name("folder-open-symbolic")
            .tooltip_text("Maximizar Tudo (Expandir todas as pastas)")
            .css_classes(["flat", "circular"])
            .build();
        files_toolbar.append(&btn_expand_all);

        let btn_refresh_tree = Button::builder()
            .icon_name("view-refresh-symbolic")
            .tooltip_text("Atualizar Árvore de Arquivos")
            .css_classes(["flat", "circular"])
            .build();
        files_toolbar.append(&btn_refresh_tree);

        files_tab_box.append(&files_toolbar);
        files_tab_box.append(&Separator::new(Orientation::Horizontal));

        let files_list = ListBox::new();
        files_list.add_css_class("navigation-sidebar");
        files_list.set_activate_on_single_click(true);
        let files_scroll = ScrolledWindow::new();
        files_scroll.set_vexpand(true);
        files_scroll.set_child(Some(&files_list));
        files_tab_box.append(&files_scroll);

        sidebar_stack.add_titled(&files_tab_box, Some("files"), "Arquivos");

        let outline_list = ListBox::new();
        outline_list.add_css_class("navigation-sidebar");
        outline_list.set_activate_on_single_click(true);
        let outline_scroll = ScrolledWindow::new();
        outline_scroll.set_child(Some(&outline_list));
        sidebar_stack.add_titled(&outline_scroll, Some("outline"), "Estrutura");

        let bib_list = ListBox::new();
        bib_list.add_css_class("navigation-sidebar");
        bib_list.set_activate_on_single_click(true);
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

        // Popovers for Toolbar Buttons:
        // Popover for creating new file at project root
        let pop_new_file = Popover::new();
        pop_new_file.set_parent(&btn_add_file);
        let pop_f_box = Box::new(Orientation::Vertical, 6);
        pop_f_box.set_margin_start(10);
        pop_f_box.set_margin_end(10);
        pop_f_box.set_margin_top(8);
        pop_f_box.set_margin_bottom(8);

        let pop_f_title = Label::new(Some("Novo Arquivo"));
        pop_f_title.add_css_class("heading");
        pop_f_title.set_xalign(0.0);
        pop_f_box.append(&pop_f_title);

        let entry_new_file = Entry::new();
        entry_new_file.set_placeholder_text(Some("ex: capitulo1.tex"));
        pop_f_box.append(&entry_new_file);

        let btn_do_create_file = Button::with_label("Criar");
        btn_do_create_file.add_css_class("suggested-action");
        pop_f_box.append(&btn_do_create_file);
        pop_new_file.set_child(Some(&pop_f_box));

        {
            let pop = pop_new_file.clone();
            btn_add_file.connect_clicked(move |_| {
                pop.popup();
            });
        }

        let do_create_root_file = {
            let pop = pop_new_file.clone();
            let ent = entry_new_file.clone();
            let st = state.clone();
            let fl = files_list.clone();
            let ce = code_editor.clone();
            let ol = outline_list.clone();
            let to = toast_overlay.clone();
            let cd = collapsed_dirs.clone();
            Rc::new(move || {
                let text = ent.text().trim().to_string();
                if !text.is_empty() {
                    let filename = if text.contains('.') {
                        text.clone()
                    } else {
                        format!("{}.tex", text)
                    };
                    let root = st.borrow().project_dir.clone();
                    let target = root.join(&filename);
                    if let Some(parent) = target.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    if !target.exists() {
                        let init_stub = if filename.ends_with(".tex") {
                            format!("% {}\n", filename)
                        } else {
                            String::new()
                        };
                        let _ = std::fs::write(&target, init_stub);
                        to.add_toast(Toast::new(&format!("Arquivo criado: {}", filename)));
                        Self::open_file_in_editor(&target, &st, &ce, &ol, Some(&to));
                        Self::refresh_file_tree_impl(&fl, &st, &ce, &ol, Some(&to), &cd);
                    } else {
                        to.add_toast(Toast::new(&format!("Arquivo já existe: {}", filename)));
                    }
                    pop.popdown();
                    ent.set_text("");
                }
            })
        };

        {
            let action = do_create_root_file.clone();
            entry_new_file.connect_activate(move |_| {
                action();
            });
        }
        {
            let action = do_create_root_file.clone();
            btn_do_create_file.connect_clicked(move |_| {
                action();
            });
        }

        // Popover for creating new folder at project root
        let pop_new_folder = Popover::new();
        pop_new_folder.set_parent(&btn_add_folder);
        let pop_d_box = Box::new(Orientation::Vertical, 6);
        pop_d_box.set_margin_start(10);
        pop_d_box.set_margin_end(10);
        pop_d_box.set_margin_top(8);
        pop_d_box.set_margin_bottom(8);

        let pop_d_title = Label::new(Some("Nova Pasta"));
        pop_d_title.add_css_class("heading");
        pop_d_title.set_xalign(0.0);
        pop_d_box.append(&pop_d_title);

        let entry_new_folder = Entry::new();
        entry_new_folder.set_placeholder_text(Some("ex: capitulos ou imagens"));
        pop_d_box.append(&entry_new_folder);

        let btn_do_create_folder = Button::with_label("Criar");
        btn_do_create_folder.add_css_class("suggested-action");
        pop_d_box.append(&btn_do_create_folder);
        pop_new_folder.set_child(Some(&pop_d_box));

        {
            let pop = pop_new_folder.clone();
            btn_add_folder.connect_clicked(move |_| {
                pop.popup();
            });
        }

        let do_create_root_folder = {
            let pop = pop_new_folder.clone();
            let ent = entry_new_folder.clone();
            let st = state.clone();
            let fl = files_list.clone();
            let ce = code_editor.clone();
            let ol = outline_list.clone();
            let to = toast_overlay.clone();
            let cd = collapsed_dirs.clone();
            Rc::new(move || {
                let text = ent.text().trim().to_string();
                if !text.is_empty() {
                    let root = st.borrow().project_dir.clone();
                    let target = root.join(&text);
                    let _ = std::fs::create_dir_all(&target);
                    to.add_toast(Toast::new(&format!("Pasta criada: {}", text)));
                    Self::refresh_file_tree_impl(&fl, &st, &ce, &ol, Some(&to), &cd);
                    pop.popdown();
                    ent.set_text("");
                }
            })
        };

        {
            let action = do_create_root_folder.clone();
            entry_new_folder.connect_activate(move |_| {
                action();
            });
        }
        {
            let action = do_create_root_folder.clone();
            btn_do_create_folder.connect_clicked(move |_| {
                action();
            });
        }

        // Refresh tree button
        {
            let fl = files_list.clone();
            let st = state.clone();
            let ce = code_editor.clone();
            let ol = outline_list.clone();
            let to = toast_overlay.clone();
            let cd = collapsed_dirs.clone();
            btn_refresh_tree.connect_clicked(move |_| {
                Self::refresh_file_tree_impl(&fl, &st, &ce, &ol, Some(&to), &cd);
                to.add_toast(Toast::new("Árvore de arquivos atualizada"));
            });
        }

        // Collapse all folders button
        {
            let st = state.clone();
            let cd = collapsed_dirs.clone();
            let fl = files_list.clone();
            let ce = code_editor.clone();
            let ol = outline_list.clone();
            let to = toast_overlay.clone();
            btn_collapse_all.connect_clicked(move |_| {
                let (project_dir, curr_file) = {
                    let s = st.borrow();
                    (s.project_dir.clone(), s.current_file.clone())
                };

                let items = FileTreeScanner::scan_project(&project_dir);
                let mut dirs = HashSet::new();
                fn collect_dirs(items: &[FileItem], dirs: &mut HashSet<PathBuf>) {
                    for it in items {
                        if it.kind == FileKind::Directory {
                            dirs.insert(it.path.clone());
                            collect_dirs(&it.children, dirs);
                        }
                    }
                }
                collect_dirs(&items, &mut dirs);
                *cd.borrow_mut() = dirs.clone();

                let proj_session = ProjectSession {
                    current_file: Some(curr_file),
                    collapsed_dirs: dirs.into_iter().collect(),
                };
                let _ = SessionManager::save_project_session(&project_dir, &proj_session);

                let mut global = SessionManager::load();
                global.projects.insert(project_dir, proj_session);
                let _ = SessionManager::save(&global);

                Self::refresh_file_tree_impl(&fl, &st, &ce, &ol, Some(&to), &cd);
                to.add_toast(Toast::new("Todas as pastas foram recolhidas."));
            });
        }

        // Expand all folders button
        {
            let st = state.clone();
            let cd = collapsed_dirs.clone();
            let fl = files_list.clone();
            let ce = code_editor.clone();
            let ol = outline_list.clone();
            let to = toast_overlay.clone();
            btn_expand_all.connect_clicked(move |_| {
                let (project_dir, curr_file) = {
                    let s = st.borrow();
                    (s.project_dir.clone(), s.current_file.clone())
                };

                cd.borrow_mut().clear();

                let proj_session = ProjectSession {
                    current_file: Some(curr_file),
                    collapsed_dirs: Vec::new(),
                };
                let _ = SessionManager::save_project_session(&project_dir, &proj_session);

                let mut global = SessionManager::load();
                global.projects.insert(project_dir, proj_session);
                let _ = SessionManager::save(&global);

                Self::refresh_file_tree_impl(&fl, &st, &ce, &ol, Some(&to), &cd);
                to.add_toast(Toast::new("Todas as pastas foram expandidas."));
            });
        }

        // 5. Center & Right Workspace:
        // Workspace Paned: [Left Area: Code + Status + Collapsible Logs] <---> [Right Area: PDF Viewer]
        let editor_pdf_paned = Paned::new(Orientation::Horizontal);
        editor_pdf_paned.set_vexpand(true);
        editor_pdf_paned.set_hexpand(true);
        editor_pdf_paned.set_position(570);

        // 6. Bottom Resizable & Minimizable Logs Drawer (Hidden by default!)
        let bottom_drawer = Box::new(Orientation::Vertical, 0);
        bottom_drawer.add_css_class("card");
        bottom_drawer.set_margin_start(8);
        bottom_drawer.set_margin_end(8);
        bottom_drawer.set_margin_bottom(4);
        bottom_drawer.set_visible(false); // Hidden by default

        let drawer_header = Box::new(Orientation::Horizontal, 8);
        drawer_header.set_margin_start(12);
        drawer_header.set_margin_end(8);
        drawer_header.set_margin_top(6);
        drawer_header.set_margin_bottom(6);

        let drawer_icon = Image::from_icon_name("utilities-terminal-symbolic");
        drawer_header.append(&drawer_icon);

        let drawer_title = Label::new(Some("Logs & Diagnóstico de Compilação"));
        drawer_title.add_css_class("heading");
        drawer_title.set_xalign(0.0);
        drawer_header.append(&drawer_title);

        let drawer_spacer = Box::new(Orientation::Horizontal, 0);
        drawer_spacer.set_hexpand(true);
        drawer_header.append(&drawer_spacer);

        let btn_clear_logs = Button::from_icon_name("edit-clear-symbolic");
        btn_clear_logs.set_tooltip_text(Some("Limpar Logs"));
        btn_clear_logs.add_css_class("flat");
        drawer_header.append(&btn_clear_logs);

        let btn_close_drawer = Button::builder()
            .icon_name("window-close-symbolic")
            .tooltip_text("Fechar Painel de Logs (Ctrl+J)")
            .css_classes(["flat"])
            .build();
        drawer_header.append(&btn_close_drawer);

        bottom_drawer.append(&drawer_header);
        bottom_drawer.append(&Separator::new(Orientation::Horizontal));

        let log_scroll = ScrolledWindow::new();
        log_scroll.set_vexpand(true);

        // Native TextView: Monospace, selectable with mouse/keyboard, copyable (Ctrl+C)
        let log_view = TextView::new();
        log_view.set_editable(false);
        log_view.set_cursor_visible(true);
        log_view.set_monospace(true);
        log_view.set_wrap_mode(WrapMode::WordChar);
        log_view.set_left_margin(12);
        log_view.set_right_margin(12);
        log_view.set_top_margin(8);
        log_view.set_bottom_margin(8);
        log_view.buffer().set_text("VibeTeX inicializado. Pronto para compilar.");
        log_scroll.set_child(Some(&log_view));
        bottom_drawer.append(&log_scroll);

        // Vertical Paned allowing the user to drag and adjust logs height when open
        let editor_logs_paned = Paned::new(Orientation::Vertical);
        editor_logs_paned.set_vexpand(true);
        editor_logs_paned.set_hexpand(true);
        editor_logs_paned.set_start_child(Some(code_editor.widget()));
        editor_logs_paned.set_end_child(Some(&bottom_drawer));
        editor_logs_paned.set_position(580);

        // 7. Bottom Status & Dock Strip (Zed-style)
        let status_strip = Box::new(Orientation::Horizontal, 8);
        status_strip.add_css_class("toolbar");
        status_strip.set_margin_start(10);
        status_strip.set_margin_end(8);
        status_strip.set_margin_top(2);
        status_strip.set_margin_bottom(2);

        let status_label = Label::new(Some("Pronto. (0 erros, 0 avisos)"));
        status_label.set_xalign(0.0);
        status_label.add_css_class("dim-label");
        status_strip.append(&status_label);

        let status_spacer = Box::new(Orientation::Horizontal, 0);
        status_spacer.set_hexpand(true);
        status_strip.append(&status_spacer);

        // Spell Checker Status Button (e.g. "✓ PT-BR" or "⚠️ 2 erros (PT-BR)")
        let btn_spell_status = Button::builder()
            .label("✓ PT-BR")
            .tooltip_text("Corretor Ortográfico (Clique para saltar para o próximo erro, atalho: F7)")
            .css_classes(["flat", "caption"])
            .build();
        status_strip.append(&btn_spell_status);

        // Spell Checker Language Selector Popover Button
        let btn_spell_lang = Button::builder()
            .icon_name("tools-check-spelling-symbolic")
            .tooltip_text("Configurar Idioma do Corretor (Português / Inglês / Bilíngue)")
            .css_classes(["flat"])
            .build();
        status_strip.append(&btn_spell_lang);

        let pop_spell_lang = Popover::builder()
            .autohide(true)
            .has_arrow(true)
            .build();
        pop_spell_lang.set_parent(&btn_spell_lang);

        let lang_menu_box = Box::new(Orientation::Vertical, 4);
        lang_menu_box.set_margin_start(8);
        lang_menu_box.set_margin_end(8);
        lang_menu_box.set_margin_top(8);
        lang_menu_box.set_margin_bottom(8);

        let lang_title = Label::new(None);
        lang_title.set_markup("<b>Idioma do Corretor</b>");
        lang_title.set_xalign(0.0);
        lang_menu_box.append(&lang_title);
        lang_menu_box.append(&Separator::new(Orientation::Horizontal));

        let opt_pt = Button::builder().label("🇧🇷 Português (Brasil)").css_classes(["flat"]).build();
        let opt_en = Button::builder().label("🇺🇸 Inglês (US)").css_classes(["flat"]).build();
        let opt_bi = Button::builder().label("🌐 Bilíngue (PT + EN)").css_classes(["flat"]).build();
        let opt_off = Button::builder().label("❌ Desativado").css_classes(["flat"]).build();

        {
            let code_c = code_editor.clone();
            let pop_c = pop_spell_lang.clone();
            opt_pt.connect_clicked(move |_| {
                code_c.set_language_mode(LanguageMode::Portuguese);
                pop_c.popdown();
            });
        }
        {
            let code_c = code_editor.clone();
            let pop_c = pop_spell_lang.clone();
            opt_en.connect_clicked(move |_| {
                code_c.set_language_mode(LanguageMode::English);
                pop_c.popdown();
            });
        }
        {
            let code_c = code_editor.clone();
            let pop_c = pop_spell_lang.clone();
            opt_bi.connect_clicked(move |_| {
                code_c.set_language_mode(LanguageMode::Bilingual);
                pop_c.popdown();
            });
        }
        {
            let code_c = code_editor.clone();
            let pop_c = pop_spell_lang.clone();
            opt_off.connect_clicked(move |_| {
                code_c.set_language_mode(LanguageMode::Disabled);
                pop_c.popdown();
            });
        }

        lang_menu_box.append(&opt_pt);
        lang_menu_box.append(&opt_en);
        lang_menu_box.append(&opt_bi);
        lang_menu_box.append(&Separator::new(Orientation::Horizontal));
        lang_menu_box.append(&opt_off);
        pop_spell_lang.set_child(Some(&lang_menu_box));

        {
            let pop_c = pop_spell_lang.clone();
            btn_spell_lang.connect_clicked(move |_| {
                pop_c.popup();
            });
        }

        // Automatic spell status update on badge
        let badge_ref = btn_spell_status.clone();
        code_editor.connect_spell_checked(move |count, mode| {
            if mode == LanguageMode::Disabled {
                badge_ref.set_label("Ortografia: OFF");
                badge_ref.remove_css_class("error");
            } else if count == 0 {
                badge_ref.set_label(&format!("✓ {}", mode.short_label()));
                badge_ref.remove_css_class("error");
            } else {
                badge_ref.set_label(&format!("⚠️ {} erros ({})", count, mode.short_label()));
                badge_ref.add_css_class("error");
            }
        });

        // Click on badge jumps to next error
        let code_jump = code_editor.clone();
        let toast_jump = toast_overlay.clone();
        btn_spell_status.connect_clicked(move |_| {
            if let Some(_) = code_jump.jump_to_next_spell_error() {
                toast_jump.add_toast(Toast::new("Saltou para o próximo erro ortográfico."));
            } else {
                toast_jump.add_toast(Toast::new("Nenhum erro ortográfico no documento!"));
            }
        });

        // Set project dictionary location
        code_editor.spell_checker().set_project_dict(&project_dir);

        // Zed-style Debug / Terminal Icon Toggle Button
        let btn_toggle_logs = ToggleButton::builder()
            .icon_name("utilities-terminal-symbolic")
            .tooltip_text("Alternar Painel de Logs / Debug (Ctrl+J)")
            .css_classes(["flat"])
            .active(false)
            .build();
        status_strip.append(&btn_toggle_logs);

        let left_editor_box = Box::new(Orientation::Vertical, 0);
        left_editor_box.set_vexpand(true);
        left_editor_box.set_hexpand(true);
        left_editor_box.append(&editor_logs_paned);
        left_editor_box.append(&status_strip);

        editor_pdf_paned.set_start_child(Some(&left_editor_box));

        // 8. PDF Viewer on the right
        let pdf_viewer = Rc::new(PdfViewerWidget::new());
        if let Some(ref p_mode) = global_session.pdf_view_mode {
            pdf_viewer.set_view_mode_str(p_mode);
        }
        if let Some(zoom) = global_session.pdf_zoom {
            pdf_viewer.set_zoom(zoom);
        }
        {
            let toast_pdf = toast_overlay.clone();
            pdf_viewer.connect_toast(move |msg| {
                toast_pdf.add_toast(Toast::new(msg));
            });
        }
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
            log_view,
            btn_auto_compile,
            btn_toggle_logs: btn_toggle_logs.clone(),
            btn_compile: btn_compile.clone(),
            collapsed_dirs,
        };

        // Save session on window close
        {
            let win_close = app_win.window.clone();
            let state_close = app_win.state.clone();
            let pdf_close = app_win.pdf_viewer.clone();
            let collapsed_close = app_win.collapsed_dirs.clone();
            let btn_code_close = btn_mode_code.clone();
            let btn_visual_close = btn_mode_visual.clone();

            app_win.window.connect_close_request(move |_| {
                let (proj_dir, curr_file) = {
                    let st = state_close.borrow();
                    (st.project_dir.clone(), st.current_file.clone())
                };

                let view_mode = if btn_code_close.is_active() {
                    "code".to_string()
                } else if btn_visual_close.is_active() {
                    "visual".to_string()
                } else {
                    "split".to_string()
                };

                let collapsed: Vec<PathBuf> = collapsed_close.borrow().iter().cloned().collect();
                let proj_session = ProjectSession {
                    current_file: Some(curr_file),
                    collapsed_dirs: collapsed,
                };

                let _ = SessionManager::save_project_session(&proj_dir, &proj_session);

                let mut global = SessionManager::load();
                global.last_project_dir = Some(proj_dir.clone());
                global.view_mode = Some(view_mode);
                global.window_width = win_close.default_width();
                global.window_height = win_close.default_height();
                global.pdf_view_mode = Some(pdf_close.view_mode_str());
                global.pdf_zoom = Some(pdf_close.zoom());
                global.projects.insert(proj_dir, proj_session);

                let _ = SessionManager::save(&global);

                glib::Propagation::Proceed
            });
        }

        app_win.refresh_file_tree();
        app_win.refresh_outline();
        app_win.refresh_bib_list();

        let btn_compile_init = btn_compile.clone();

        app_win.setup_event_handlers(
            btn_open_proj,
            btn_save,
            btn_compile,
            btn_undo,
            btn_redo,
            btn_new_proj,
            pop_export,
            btn_export_pdf,
            btn_export_zip,
            btn_clear_logs,
            btn_toggle_sidebar,
            btn_toggle_logs,
            btn_close_drawer,
            btn_spell_status,
            bottom_drawer,
            sidebar_box,
            root_paned,
            editor_logs_paned,
            editor_pdf_paned,
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
        Self::refresh_file_tree_impl(
            &self.files_list,
            &self.state,
            &self.code_editor,
            &self.outline_list,
            Some(&self.toast_overlay),
            &self.collapsed_dirs,
        );
    }

    pub fn refresh_outline(&self) {
        Self::refresh_outline_impl(&self.outline_list, &self.state, &self.code_editor);
    }

    pub fn refresh_bib_list(&self) {
        Self::refresh_bib_list_impl(&self.bib_list, &self.state, &self.code_editor);
    }

    #[allow(dead_code)]
    pub fn open_project_directory(&self, new_dir: &Path) {
        Self::open_project_static(
            &self.window,
            &self.toast_overlay,
            &self.code_editor,
            &self.pdf_viewer,
            &self.state,
            &self.files_list,
            &self.outline_list,
            &self.bib_list,
            &self.btn_compile,
            &self.collapsed_dirs,
            new_dir,
        );
    }

    fn open_file_in_editor(
        path: &Path,
        state: &Rc<RefCell<AppState>>,
        code_editor: &Rc<CodeEditorWidget>,
        outline_list: &ListBox,
        toast_overlay: Option<&ToastOverlay>,
    ) {
        if !path.is_file() {
            return;
        }

        // 1. Save currently open file to disk first so no changes are lost
        {
            let st = state.borrow();
            let _ = ProjectManager::atomic_write(&st.current_file, st.document.source().as_bytes());
        }

        // 2. Read new file content and switch
        match std::fs::read_to_string(path) {
            Ok(content) => {
                let project_dir = {
                    let mut s = state.borrow_mut();
                    s.current_file = path.to_path_buf();
                    s.document = SharedDocument::new(&content);
                    s.project_dir.clone()
                };
                code_editor.set_text_silently(&content);
                Self::refresh_outline_impl(outline_list, state, code_editor);

                // Persist current_file in project session
                let mut global = SessionManager::load();
                let mut proj_session = SessionManager::load_project_session(&project_dir, &global);
                proj_session.current_file = Some(path.to_path_buf());
                let _ = SessionManager::save_project_session(&project_dir, &proj_session);
                global.projects.insert(project_dir, proj_session);
                let _ = SessionManager::save(&global);

                if let Some(toast) = toast_overlay {
                    let name = path.file_name().unwrap_or_default().to_string_lossy();
                    toast.add_toast(Toast::new(&format!("Arquivo aberto: {}", name)));
                }
            }
            Err(err) => {
                if let Some(toast) = toast_overlay {
                    toast.add_toast(Toast::new(&format!("Não foi possível abrir como texto: {}", err)));
                }
            }
        }
    }

    fn refresh_file_tree_impl(
        files_list: &ListBox,
        state: &Rc<RefCell<AppState>>,
        code_editor: &Rc<CodeEditorWidget>,
        outline_list: &ListBox,
        toast_overlay: Option<&ToastOverlay>,
        collapsed_dirs: &Rc<RefCell<HashSet<PathBuf>>>,
    ) {
        while let Some(child) = files_list.first_child() {
            files_list.remove(&child);
        }

        let st = state.borrow();
        let items = FileTreeScanner::scan_project(&st.project_dir);

        Self::render_tree_items(
            &items,
            0,
            files_list,
            state,
            code_editor,
            outline_list,
            toast_overlay,
            collapsed_dirs,
        );
    }

    fn render_tree_items(
        items: &[FileItem],
        depth: u32,
        files_list: &ListBox,
        state: &Rc<RefCell<AppState>>,
        code_editor: &Rc<CodeEditorWidget>,
        outline_list: &ListBox,
        toast_overlay: Option<&ToastOverlay>,
        collapsed_dirs: &Rc<RefCell<HashSet<PathBuf>>>,
    ) {
        let current_file = {
            let st = state.borrow();
            st.current_file.clone()
        };

        for item in items {
            let row = ListBoxRow::new();
            row.set_activatable(true);

            let hbox = Box::new(Orientation::Horizontal, 4);
            hbox.set_margin_top(2);
            hbox.set_margin_bottom(2);
            hbox.set_margin_start(((depth * 14) + 6) as i32);
            hbox.set_margin_end(6);

            let click_target = Box::new(Orientation::Horizontal, 6);
            click_target.set_hexpand(true);

            if item.kind == FileKind::Directory {
                let is_collapsed = collapsed_dirs.borrow().contains(&item.path);

                // Expand / Collapse Arrow Icon
                let arrow_icon = Image::from_icon_name(if is_collapsed {
                    "pan-end-symbolic"
                } else {
                    "pan-down-symbolic"
                });
                arrow_icon.set_pixel_size(12);
                click_target.append(&arrow_icon);

                // Folder Icon
                let folder_icon = Image::from_icon_name(if is_collapsed {
                    "folder-symbolic"
                } else {
                    "folder-open-symbolic"
                });
                click_target.append(&folder_icon);

                // Folder Name Label
                let label = Label::new(Some(&item.name));
                label.set_xalign(0.0);
                label.add_css_class("heading");
                label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
                click_target.append(&label);

                hbox.append(&click_target);

                // Action 1: Add File or Subfolder in this directory
                let btn_add = Button::builder()
                    .icon_name("list-add-symbolic")
                    .tooltip_text("Criar arquivo ou pasta aqui...")
                    .css_classes(["flat", "circular"])
                    .build();

                let pop_add = Popover::new();
                pop_add.set_parent(&btn_add);

                let pop_box = Box::new(Orientation::Vertical, 6);
                pop_box.set_margin_start(10);
                pop_box.set_margin_end(10);
                pop_box.set_margin_top(8);
                pop_box.set_margin_bottom(8);

                let pop_title = Label::new(Some(&format!("Criar em {}", item.name)));
                pop_title.add_css_class("heading");
                pop_title.set_xalign(0.0);
                pop_box.append(&pop_title);

                let entry_name = Entry::new();
                entry_name.set_placeholder_text(Some("Nome (ex: intro.tex ou imagens)"));
                pop_box.append(&entry_name);

                let actions_box = Box::new(Orientation::Horizontal, 6);

                let btn_create_file = Button::with_label("+ Arquivo");
                btn_create_file.add_css_class("suggested-action");
                actions_box.append(&btn_create_file);

                let btn_create_folder = Button::with_label("+ Pasta");
                actions_box.append(&btn_create_folder);

                pop_box.append(&actions_box);
                pop_add.set_child(Some(&pop_box));

                let pop_clone = pop_add.clone();
                btn_add.connect_clicked(move |_| {
                    pop_clone.popup();
                });

                // Enter inside entry activates + Arquivo
                {
                    let bcf = btn_create_file.clone();
                    entry_name.connect_activate(move |_| {
                        bcf.emit_clicked();
                    });
                }

                // Handler: + Arquivo
                {
                    let pop = pop_add.clone();
                    let ent = entry_name.clone();
                    let dir_path = item.path.clone();
                    let fl = files_list.clone();
                    let st = state.clone();
                    let ce = code_editor.clone();
                    let ol = outline_list.clone();
                    let to = toast_overlay.cloned();
                    let cd = collapsed_dirs.clone();
                    btn_create_file.connect_clicked(move |_| {
                        let text = ent.text().trim().to_string();
                        if !text.is_empty() {
                            let filename = if text.contains('.') {
                                text.clone()
                            } else {
                                format!("{}.tex", text)
                            };
                            let target = dir_path.join(&filename);
                            if let Some(parent) = target.parent() {
                                let _ = std::fs::create_dir_all(parent);
                            }
                            if !target.exists() {
                                let init_stub = if filename.ends_with(".tex") {
                                    format!("% {}\n", filename)
                                } else {
                                    String::new()
                                };
                                let _ = std::fs::write(&target, init_stub);
                                if let Some(ref t) = to {
                                    t.add_toast(Toast::new(&format!("Arquivo criado: {}", filename)));
                                }
                                cd.borrow_mut().remove(&dir_path);
                                Self::open_file_in_editor(&target, &st, &ce, &ol, to.as_ref());
                                Self::refresh_file_tree_impl(&fl, &st, &ce, &ol, to.as_ref(), &cd);
                            } else if let Some(ref t) = to {
                                t.add_toast(Toast::new(&format!("Arquivo já existe: {}", filename)));
                            }
                            pop.popdown();
                            ent.set_text("");
                        }
                    });
                }

                // Handler: + Pasta
                {
                    let pop = pop_add.clone();
                    let ent = entry_name.clone();
                    let dir_path = item.path.clone();
                    let fl = files_list.clone();
                    let st = state.clone();
                    let ce = code_editor.clone();
                    let ol = outline_list.clone();
                    let to = toast_overlay.cloned();
                    let cd = collapsed_dirs.clone();
                    btn_create_folder.connect_clicked(move |_| {
                        let text = ent.text().trim().to_string();
                        if !text.is_empty() {
                            let target = dir_path.join(&text);
                            let _ = std::fs::create_dir_all(&target);
                            if let Some(ref t) = to {
                                t.add_toast(Toast::new(&format!("Pasta criada: {}", text)));
                            }
                            cd.borrow_mut().remove(&dir_path);
                            Self::refresh_file_tree_impl(&fl, &st, &ce, &ol, to.as_ref(), &cd);
                            pop.popdown();
                            ent.set_text("");
                        }
                    });
                }

                hbox.append(&btn_add);

                // Action 2: Delete Folder with confirmation
                let btn_del = Button::builder()
                    .icon_name("user-trash-symbolic")
                    .tooltip_text("Excluir pasta...")
                    .css_classes(["flat", "circular"])
                    .build();

                let pop_del = Popover::new();
                pop_del.set_parent(&btn_del);

                let del_box = Box::new(Orientation::Vertical, 8);
                del_box.set_margin_start(10);
                del_box.set_margin_end(10);
                del_box.set_margin_top(8);
                del_box.set_margin_bottom(8);

                let del_title = Label::new(Some(&format!("Excluir pasta '{}'?", item.name)));
                del_title.add_css_class("heading");
                del_title.set_xalign(0.0);
                del_box.append(&del_title);

                let del_msg = Label::new(Some("A pasta e todo o seu conteúdo serão removidos permanentemente."));
                del_msg.add_css_class("dim-label");
                del_msg.set_wrap(true);
                del_msg.set_max_width_chars(25);
                del_box.append(&del_msg);

                let del_btns = Box::new(Orientation::Horizontal, 8);
                let btn_cancel = Button::with_label("Cancelar");
                let btn_confirm_del = Button::with_label("Excluir");
                btn_confirm_del.add_css_class("destructive-action");
                del_btns.append(&btn_cancel);
                del_btns.append(&btn_confirm_del);
                del_box.append(&del_btns);

                pop_del.set_child(Some(&del_box));

                let pop_del_clone = pop_del.clone();
                btn_del.connect_clicked(move |_| {
                    pop_del_clone.popup();
                });

                {
                    let pop = pop_del.clone();
                    btn_cancel.connect_clicked(move |_| {
                        pop.popdown();
                    });
                }

                {
                    let pop = pop_del.clone();
                    let target = item.path.clone();
                    let name_str = item.name.clone();
                    let fl = files_list.clone();
                    let st = state.clone();
                    let ce = code_editor.clone();
                    let ol = outline_list.clone();
                    let to = toast_overlay.cloned();
                    let cd = collapsed_dirs.clone();
                    btn_confirm_del.connect_clicked(move |_| {
                        let is_current_inside = {
                            let s = st.borrow();
                            s.current_file == target || s.current_file.starts_with(&target)
                        };
                        let _ = std::fs::remove_dir_all(&target);
                        if let Some(ref t) = to {
                            t.add_toast(Toast::new(&format!("Pasta excluída: {}", name_str)));
                        }
                        pop.popdown();

                        if is_current_inside {
                            let root = st.borrow().project_dir.clone();
                            let main_path = root.join("main.tex");
                            if main_path.exists() {
                                Self::open_file_in_editor(&main_path, &st, &ce, &ol, to.as_ref());
                            } else if let Some(another) = FileTreeScanner::scan_flat_files(&root)
                                .into_iter()
                                .find(|f| f.kind == FileKind::Latex)
                            {
                                Self::open_file_in_editor(&another.path, &st, &ce, &ol, to.as_ref());
                            } else {
                                ce.set_text_silently("");
                            }
                        }
                        cd.borrow_mut().remove(&target);
                        Self::refresh_file_tree_impl(&fl, &st, &ce, &ol, to.as_ref(), &cd);
                    });
                }

                hbox.append(&btn_del);

                // Clicking directory toggles collapse
                let folder_path = item.path.clone();
                let fl = files_list.clone();
                let st = state.clone();
                let ce = code_editor.clone();
                let ol = outline_list.clone();
                let to = toast_overlay.cloned();
                let cd = collapsed_dirs.clone();

                let gesture = GestureClick::new();
                let fp_g = folder_path.clone();
                let fl_g = fl.clone();
                let st_g = st.clone();
                let ce_g = ce.clone();
                let ol_g = ol.clone();
                let to_g = to.clone();
                let cd_g = cd.clone();
                gesture.connect_pressed(move |_, n_press, _, _| {
                    if n_press == 1 {
                        let mut set = cd_g.borrow_mut();
                        if set.contains(&fp_g) {
                            set.remove(&fp_g);
                        } else {
                            set.insert(fp_g.clone());
                        }
                        let collapsed_vec: Vec<PathBuf> = set.iter().cloned().collect();
                        drop(set);

                        let (p_dir, c_file) = {
                            let s = st_g.borrow();
                            (s.project_dir.clone(), s.current_file.clone())
                        };
                        let proj_sess = ProjectSession {
                            current_file: Some(c_file),
                            collapsed_dirs: collapsed_vec,
                        };
                        let _ = SessionManager::save_project_session(&p_dir, &proj_sess);

                        Self::refresh_file_tree_impl(&fl_g, &st_g, &ce_g, &ol_g, to_g.as_ref(), &cd_g);
                    }
                });
                click_target.add_controller(gesture);

                let key_ctrl = EventControllerKey::new();
                let fp_k = folder_path.clone();
                let fl_k = fl.clone();
                let st_k = st.clone();
                let ce_k = ce.clone();
                let ol_k = ol.clone();
                let to_k = to.clone();
                let cd_k = cd.clone();
                key_ctrl.connect_key_pressed(move |_, key, _, _| {
                    if key == Key::Return || key == Key::KP_Enter || key == Key::space {
                        let mut set = cd_k.borrow_mut();
                        if set.contains(&fp_k) {
                            set.remove(&fp_k);
                        } else {
                            set.insert(fp_k.clone());
                        }
                        let collapsed_vec: Vec<PathBuf> = set.iter().cloned().collect();
                        drop(set);

                        let (p_dir, c_file) = {
                            let s = st_k.borrow();
                            (s.project_dir.clone(), s.current_file.clone())
                        };
                        let proj_sess = ProjectSession {
                            current_file: Some(c_file),
                            collapsed_dirs: collapsed_vec,
                        };
                        let _ = SessionManager::save_project_session(&p_dir, &proj_sess);

                        Self::refresh_file_tree_impl(&fl_k, &st_k, &ce_k, &ol_k, to_k.as_ref(), &cd_k);
                        glib::Propagation::Stop
                    } else {
                        glib::Propagation::Proceed
                    }
                });
                row.add_controller(key_ctrl);

                row.set_child(Some(&hbox));
                files_list.append(&row);

                if !is_collapsed {
                    Self::render_tree_items(
                        &item.children,
                        depth + 1,
                        files_list,
                        state,
                        code_editor,
                        outline_list,
                        toast_overlay,
                        collapsed_dirs,
                    );
                }
            } else {
                // File Row
                let is_current = current_file == item.path;

                // Indent spacer to align file icon with folder icon
                let indent_spacer = Box::new(Orientation::Horizontal, 0);
                indent_spacer.set_width_request(12);
                click_target.append(&indent_spacer);

                let file_icon = Image::from_icon_name(item.kind.icon_name());
                click_target.append(&file_icon);

                let label = Label::new(Some(&item.name));
                label.set_xalign(0.0);
                if is_current {
                    label.add_css_class("heading");
                    row.add_css_class("selected");
                }
                label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
                click_target.append(&label);

                hbox.append(&click_target);

                // Delete button for file
                let btn_del = Button::builder()
                    .icon_name("user-trash-symbolic")
                    .tooltip_text("Excluir arquivo...")
                    .css_classes(["flat", "circular"])
                    .build();

                let pop_del = Popover::new();
                pop_del.set_parent(&btn_del);

                let del_box = Box::new(Orientation::Vertical, 8);
                del_box.set_margin_start(10);
                del_box.set_margin_end(10);
                del_box.set_margin_top(8);
                del_box.set_margin_bottom(8);

                let del_title = Label::new(Some(&format!("Excluir '{}'?", item.name)));
                del_title.add_css_class("heading");
                del_title.set_xalign(0.0);
                del_box.append(&del_title);

                let del_msg = Label::new(Some("Esta ação não pode ser desfeita."));
                del_msg.add_css_class("dim-label");
                del_box.append(&del_msg);

                let del_btns = Box::new(Orientation::Horizontal, 8);
                let btn_cancel = Button::with_label("Cancelar");
                let btn_confirm_del = Button::with_label("Excluir");
                btn_confirm_del.add_css_class("destructive-action");
                del_btns.append(&btn_cancel);
                del_btns.append(&btn_confirm_del);
                del_box.append(&del_btns);

                pop_del.set_child(Some(&del_box));

                let pop_del_clone = pop_del.clone();
                btn_del.connect_clicked(move |_| {
                    pop_del_clone.popup();
                });

                {
                    let pop = pop_del.clone();
                    btn_cancel.connect_clicked(move |_| {
                        pop.popdown();
                    });
                }

                {
                    let pop = pop_del.clone();
                    let target = item.path.clone();
                    let name_str = item.name.clone();
                    let fl = files_list.clone();
                    let st = state.clone();
                    let ce = code_editor.clone();
                    let ol = outline_list.clone();
                    let to = toast_overlay.cloned();
                    let cd = collapsed_dirs.clone();
                    btn_confirm_del.connect_clicked(move |_| {
                        let is_current = {
                            let s = st.borrow();
                            s.current_file == target
                        };
                        let _ = std::fs::remove_file(&target);
                        if let Some(ref t) = to {
                            t.add_toast(Toast::new(&format!("Arquivo excluído: {}", name_str)));
                        }
                        pop.popdown();

                        if is_current {
                            let root = st.borrow().project_dir.clone();
                            let main_path = root.join("main.tex");
                            if main_path.exists() {
                                Self::open_file_in_editor(&main_path, &st, &ce, &ol, to.as_ref());
                            } else if let Some(another) = FileTreeScanner::scan_flat_files(&root)
                                .into_iter()
                                .find(|f| f.kind == FileKind::Latex)
                            {
                                Self::open_file_in_editor(&another.path, &st, &ce, &ol, to.as_ref());
                            } else {
                                ce.set_text_silently("");
                            }
                        }
                        Self::refresh_file_tree_impl(&fl, &st, &ce, &ol, to.as_ref(), &cd);
                    });
                }

                hbox.append(&btn_del);

                // Clicking file opens in editor
                let file_path = item.path.clone();
                let st = state.clone();
                let ce = code_editor.clone();
                let ol = outline_list.clone();
                let to = toast_overlay.cloned();

                let gesture = GestureClick::new();
                let fp_g = file_path.clone();
                let st_g = st.clone();
                let ce_g = ce.clone();
                let ol_g = ol.clone();
                let to_g = to.clone();
                gesture.connect_pressed(move |_, n_press, _, _| {
                    if n_press == 1 {
                        Self::open_file_in_editor(&fp_g, &st_g, &ce_g, &ol_g, to_g.as_ref());
                    }
                });
                click_target.add_controller(gesture);

                let key_ctrl = EventControllerKey::new();
                let fp_k = file_path.clone();
                let st_k = st.clone();
                let ce_k = ce.clone();
                let ol_k = ol.clone();
                let to_k = to.clone();
                key_ctrl.connect_key_pressed(move |_, key, _, _| {
                    if key == Key::Return || key == Key::KP_Enter {
                        Self::open_file_in_editor(&fp_k, &st_k, &ce_k, &ol_k, to_k.as_ref());
                        glib::Propagation::Stop
                    } else {
                        glib::Propagation::Proceed
                    }
                });
                row.add_controller(key_ctrl);

                row.set_child(Some(&hbox));
                files_list.append(&row);
            }
        }
    }

    fn refresh_outline_impl(
        outline_list: &ListBox,
        state: &Rc<RefCell<AppState>>,
        code_editor: &Rc<CodeEditorWidget>,
    ) {
        while let Some(child) = outline_list.first_child() {
            outline_list.remove(&child);
        }

        let st = state.borrow();
        let ast = st.document.ast();
        let mut labels = Vec::new();

        for node in &ast.nodes {
            match node {
                AstNode::Section { level, title, title_span, .. } => {
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
                    let code_clone = code_editor.clone();

                    let gesture = GestureClick::new();
                    let code_g = code_clone.clone();
                    gesture.connect_pressed(move |_, n_press, _, _| {
                        if n_press == 1 {
                            code_g.set_cursor_offset(offset);
                        }
                    });
                    row.add_controller(gesture);

                    row.connect_activated(move |_| {
                        code_clone.set_cursor_offset(offset);
                    });

                    outline_list.append(&row);
                }
                AstNode::Paragraph { inlines, .. } => {
                    for inline in inlines {
                        if let InlineNode::Label { key, .. } = inline {
                            if !labels.contains(key) {
                                labels.push(key.clone());
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        // Fast text scan for all \label{...} in document source
        let src = st.document.source();
        let mut cursor = 0;
        while let Some(pos) = src[cursor..].find(r"\label{") {
            let start = cursor + pos + 7;
            if let Some(end) = src[start..].find('}') {
                let lbl = src[start..start + end].trim();
                if !lbl.is_empty() && !labels.iter().any(|l| l == lbl) {
                    labels.push(lbl.to_string());
                }
                cursor = start + end + 1;
            } else {
                break;
            }
        }

        code_editor.set_labels(labels);
    }

    fn refresh_bib_list_impl(
        bib_list: &ListBox,
        state: &Rc<RefCell<AppState>>,
        code_editor: &Rc<CodeEditorWidget>,
    ) {
        while let Some(child) = bib_list.first_child() {
            bib_list.remove(&child);
        }

        let st = state.borrow();
        let entries = st.bib_index.all_entries();

        if entries.is_empty() {
            let row = ActionRow::new();
            row.set_title("Nenhuma referência .bib carregada");
            bib_list.append(&row);
            code_editor.set_bib_suggestions(Vec::new());
            return;
        }

        let suggestions: Vec<BibSuggestion> = entries
            .iter()
            .map(|e| BibSuggestion {
                cite_key: e.cite_key.clone(),
                title: e.fields.get("title").cloned().unwrap_or_default(),
                author: e.author().to_string(),
                year: e.year().to_string(),
            })
            .collect();
        code_editor.set_bib_suggestions(suggestions);

        for entry in entries {
            let row = ActionRow::new();
            row.set_title(&entry.cite_key);
            row.set_subtitle(&format!("{} ({})", entry.author(), entry.year()));
            row.set_icon_name(Some("x-office-address-book-symbolic"));
            row.set_activatable(true);

            let key_clone = entry.cite_key.clone();
            let code_clone = code_editor.clone();
            let state_clone = state.clone();

            let gesture = GestureClick::new();
            let key_g = key_clone.clone();
            let code_g = code_clone.clone();
            let state_g = state_clone.clone();
            gesture.connect_pressed(move |_, n_press, _, _| {
                if n_press == 1 {
                    let cursor = code_g.get_cursor_offset();
                    let delta = DocumentOperations::insert_citation(cursor, &key_g, ChangeOrigin::CodeEditor);
                    let mut s = state_g.borrow_mut();
                    if let Ok(_) = s.document.apply_delta(delta) {
                        let updated = s.document.source().to_string();
                        code_g.set_text_silently(&updated);
                    }
                }
            });
            row.add_controller(gesture);

            row.connect_activated(move |_| {
                let cursor = code_clone.get_cursor_offset();
                let delta = DocumentOperations::insert_citation(cursor, &key_clone, ChangeOrigin::CodeEditor);
                let mut s = state_clone.borrow_mut();
                if let Ok(_) = s.document.apply_delta(delta) {
                    let updated = s.document.source().to_string();
                    code_clone.set_text_silently(&updated);
                }
            });

            bib_list.append(&row);
        }
    }

    pub fn open_project_static(
        window: &ApplicationWindow,
        toast_overlay: &ToastOverlay,
        code_editor: &Rc<CodeEditorWidget>,
        pdf_viewer: &Rc<PdfViewerWidget>,
        state: &Rc<RefCell<AppState>>,
        files_list: &ListBox,
        outline_list: &ListBox,
        bib_list: &ListBox,
        btn_compile: &Button,
        collapsed_dirs: &Rc<RefCell<HashSet<PathBuf>>>,
        new_dir: &Path,
    ) {
        if !new_dir.is_dir() {
            return;
        }

        // 1. Save and update last_project_dir
        let mut global = SessionManager::load();
        global.last_project_dir = Some(new_dir.to_path_buf());
        let _ = SessionManager::save(&global);

        let proj_session = SessionManager::load_project_session(new_dir, &global);

        let main_file_path = if let Some(ref saved_file) = proj_session.current_file {
            if saved_file.exists() && saved_file.is_file() {
                saved_file.clone()
            } else {
                new_dir.join(ProjectManager::detect_main_file(new_dir))
            }
        } else {
            new_dir.join(ProjectManager::detect_main_file(new_dir))
        };

        let initial_text = if main_file_path.exists() {
            std::fs::read_to_string(&main_file_path).unwrap_or_else(|_| Self::default_initial_latex())
        } else {
            Self::default_initial_latex()
        };

        let mut bib_index = BibIndex::new();
        if let Ok(entries) = std::fs::read_dir(new_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().map(|e| e == "bib").unwrap_or(false) {
                    if let Ok(content) = std::fs::read_to_string(&p) {
                        bib_index.load_bib_file(&content);
                    }
                }
            }
        }

        {
            let mut s = state.borrow_mut();
            s.project_dir = new_dir.to_path_buf();
            s.current_file = main_file_path;
            s.document = SharedDocument::new(&initial_text);
            s.bib_index = bib_index;
            s.is_compiling = false;
        }

        code_editor.spell_checker().set_project_dict(new_dir);
        code_editor.set_text_silently(&initial_text);
        *collapsed_dirs.borrow_mut() = proj_session.collapsed_dirs.into_iter().collect();
        Self::refresh_file_tree_impl(files_list, state, code_editor, outline_list, Some(toast_overlay), collapsed_dirs);
        Self::refresh_outline_impl(outline_list, state, code_editor);
        Self::refresh_bib_list_impl(bib_list, state, code_editor);

        let dir_name = new_dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Projeto LaTeX".to_string());

        window.set_title(Some(&format!("VibeTeX — {}", dir_name)));
        toast_overlay.add_toast(Toast::new(&format!("Projeto aberto: {}", dir_name)));

        let main_file_rel = ProjectManager::detect_main_file(new_dir);
        let cfg = ProjectConfig::new(&dir_name, &main_file_rel);
        let expected_pdf = CompilerRunner::get_pdf_path(new_dir, &cfg);
        if expected_pdf.exists() {
            pdf_viewer.load_pdf(&expected_pdf);
        } else {
            btn_compile.emit_clicked();
        }
    }

    fn setup_event_handlers(
        &self,
        btn_open_proj: Button,
        btn_save: Button,
        btn_compile: Button,
        btn_undo: Button,
        btn_redo: Button,
        btn_new_proj: Button,
        pop_export: Popover,
        btn_export_pdf: Button,
        btn_export_zip: Button,
        btn_clear_logs: Button,
        btn_toggle_sidebar: Button,
        btn_toggle_logs: ToggleButton,
        btn_close_drawer: Button,
        btn_spell_status: Button,
        bottom_drawer: Box,
        sidebar_box: Box,
        root_paned: Paned,
        editor_logs_paned: Paned,
        editor_pdf_paned: Paned,
        btn_mode_code: ToggleButton,
        btn_mode_visual: ToggleButton,
        btn_mode_split: ToggleButton,
    ) {
        // --- 1. View Mode Switching: [Código] [Ambos] [Visual] ---
        // Use the whole left column (editor + log drawer + status strip) so that
        // nothing from the code side remains visible in PDF-only mode.
        let left_panel: gtk4::Widget = editor_pdf_paned
            .start_child()
            .unwrap_or_else(|| editor_logs_paned.clone().upcast());
        let right_panel = self.pdf_viewer.widget().clone();
        let ws_paned = editor_pdf_paned.clone();

        btn_mode_code.connect_toggled({
            let left_p = left_panel.clone();
            let right_p = right_panel.clone();
            move |btn| {
                if btn.is_active() {
                    left_p.set_visible(true);
                    right_p.set_visible(false);
                    let mut g = SessionManager::load();
                    g.view_mode = Some("code".to_string());
                    let _ = SessionManager::save(&g);
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
                    let mut g = SessionManager::load();
                    g.view_mode = Some("visual".to_string());
                    let _ = SessionManager::save(&g);
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
                    let mut g = SessionManager::load();
                    g.view_mode = Some("split".to_string());
                    let _ = SessionManager::save(&g);
                }
            }
        });

        // Apply initial layout visibility based on active view mode
        if btn_mode_code.is_active() {
            left_panel.set_visible(true);
            right_panel.set_visible(false);
        } else if btn_mode_visual.is_active() {
            left_panel.set_visible(false);
            right_panel.set_visible(true);
        } else {
            left_panel.set_visible(true);
            right_panel.set_visible(true);
            ws_paned.set_position(570);
        }

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

        // --- 3. Zed-Style Debug / Logs Drawer Toggle & Close ---
        let drawer_ref = bottom_drawer.clone();
        let logs_paned_ref = editor_logs_paned.clone();
        let saved_drawer_pos = Rc::new(Cell::new(560));
        let btn_toggle_ref = btn_toggle_logs.clone();

        btn_toggle_logs.connect_toggled({
            let d = drawer_ref.clone();
            let p = logs_paned_ref.clone();
            let s = saved_drawer_pos.clone();
            move |btn| {
                let active = btn.is_active();
                d.set_visible(active);
                if active {
                    p.set_position(s.get());
                }
            }
        });

        btn_close_drawer.connect_clicked({
            let b = btn_toggle_ref.clone();
            move |_| {
                b.set_active(false);
            }
        });

        let saved_pos_track = saved_drawer_pos.clone();
        let drawer_vis_check = bottom_drawer.clone();
        editor_logs_paned.connect_position_notify(move |paned| {
            if drawer_vis_check.is_visible() && paned.position() > 100 {
                saved_pos_track.set(paned.position());
            }
        });

        // --- 4. Open Project Dialog & Keyboard Shortcuts ---
        let win_ref_open = self.window.clone();
        let toast_ref_open = self.toast_overlay.clone();
        let code_ref_open = self.code_editor.clone();
        let pdf_ref_open = self.pdf_viewer.clone();
        let st_ref_open = self.state.clone();
        let files_ref_open = self.files_list.clone();
        let outline_ref_open = self.outline_list.clone();
        let bib_ref_open = self.bib_list.clone();
        let comp_ref_open = btn_compile.clone();
        let collapsed_ref_open = self.collapsed_dirs.clone();

        let open_action = Rc::new(move || {
            let chooser = FileChooserNative::new(
                Some("Abrir Pasta do Projeto LaTeX"),
                Some(&win_ref_open),
                FileChooserAction::SelectFolder,
                Some("Abrir"),
                Some("Cancelar"),
            );
            chooser.set_modal(true);

            let win_c = win_ref_open.clone();
            let toast_c = toast_ref_open.clone();
            let code_c = code_ref_open.clone();
            let pdf_c = pdf_ref_open.clone();
            let st_c = st_ref_open.clone();
            let files_c = files_ref_open.clone();
            let outline_c = outline_ref_open.clone();
            let bib_c = bib_ref_open.clone();
            let comp_c = comp_ref_open.clone();
            let collapsed_c = collapsed_ref_open.clone();

            chooser.connect_response(move |dialog, response| {
                if response == ResponseType::Accept {
                    if let Some(file) = dialog.file() {
                        if let Some(path) = file.path() {
                            Self::open_project_static(
                                &win_c,
                                &toast_c,
                                &code_c,
                                &pdf_c,
                                &st_c,
                                &files_c,
                                &outline_c,
                                &bib_c,
                                &comp_c,
                                &collapsed_c,
                                &path,
                            );
                        }
                    }
                }
            });

            chooser.show();
        });

        let open_click = open_action.clone();
        btn_open_proj.connect_clicked(move |_| {
            open_click();
        });

        // --- 4. Export Actions (PDF and ZIP) ---
        let win_pdf = self.window.clone();
        let state_pdf = self.state.clone();
        let toast_pdf = self.toast_overlay.clone();
        let pop_pdf = pop_export.clone();

        let export_pdf_action = Rc::new(move || {
            pop_pdf.popdown();
            let (proj_dir, dir_name) = {
                let st = state_pdf.borrow();
                let name = st
                    .project_dir
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("documento")
                    .to_string();
                (st.project_dir.clone(), name)
            };

            let main_file_rel = ProjectManager::detect_main_file(&proj_dir);
            let cfg = ProjectConfig::new("VibeTeX Project", &main_file_rel);
            let compiled_pdf = CompilerRunner::get_pdf_path(&proj_dir, &cfg);

            if !compiled_pdf.exists() {
                toast_pdf.add_toast(Toast::new(
                    "Nenhum PDF compilado disponível. Compile o projeto primeiro (Ctrl+B).",
                ));
                return;
            }

            let default_name = format!("{}.pdf", dir_name);
            let chooser = FileChooserNative::new(
                Some("Exportar Documento PDF"),
                Some(&win_pdf),
                FileChooserAction::Save,
                Some("Exportar"),
                Some("Cancelar"),
            );
            chooser.set_modal(true);
            chooser.set_current_name(&default_name);

            let filter = FileFilter::new();
            filter.set_name(Some("Documento PDF (*.pdf)"));
            filter.add_mime_type("application/pdf");
            filter.add_pattern("*.pdf");
            chooser.add_filter(&filter);

            let toast_c = toast_pdf.clone();
            chooser.connect_response(move |dialog, response| {
                if response == ResponseType::Accept {
                    if let Some(target_file) = dialog.file() {
                        if let Some(target_path) = target_file.path() {
                            let final_path = if target_path.extension().map(|e| e == "pdf").unwrap_or(false) {
                                target_path
                            } else {
                                target_path.with_extension("pdf")
                            };

                            match std::fs::copy(&compiled_pdf, &final_path) {
                                Ok(_) => {
                                    toast_c.add_toast(Toast::new(&format!(
                                        "PDF exportado com sucesso: {}",
                                        final_path.file_name().unwrap_or_default().to_string_lossy()
                                    )));
                                }
                                Err(e) => {
                                    toast_c.add_toast(Toast::new(&format!("Erro ao exportar PDF: {}", e)));
                                }
                            }
                        }
                    }
                }
            });

            chooser.show();
        });

        let exp_pdf_click = export_pdf_action.clone();
        btn_export_pdf.connect_clicked(move |_| {
            exp_pdf_click();
        });

        let exp_pdf_viewer = export_pdf_action.clone();
        self.pdf_viewer.connect_export_pdf(move || {
            exp_pdf_viewer();
        });

        let win_zip = self.window.clone();
        let state_zip = self.state.clone();
        let toast_zip = self.toast_overlay.clone();
        let pop_zip = pop_export.clone();

        let export_zip_action = Rc::new(move || {
            pop_zip.popdown();
            let (proj_dir, dir_name) = {
                let st = state_zip.borrow();
                let name = st
                    .project_dir
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("projeto_latex")
                    .to_string();
                (st.project_dir.clone(), name)
            };

            let default_name = format!("{}.zip", dir_name);
            let chooser = FileChooserNative::new(
                Some("Exportar Projeto LaTeX (.zip)"),
                Some(&win_zip),
                FileChooserAction::Save,
                Some("Exportar"),
                Some("Cancelar"),
            );
            chooser.set_modal(true);
            chooser.set_current_name(&default_name);

            let filter = FileFilter::new();
            filter.set_name(Some("Arquivo ZIP (*.zip)"));
            filter.add_mime_type("application/zip");
            filter.add_pattern("*.zip");
            chooser.add_filter(&filter);

            let toast_c = toast_zip.clone();
            chooser.connect_response(move |dialog, response| {
                if response == ResponseType::Accept {
                    if let Some(target_file) = dialog.file() {
                        if let Some(target_path) = target_file.path() {
                            let final_path = if target_path.extension().map(|e| e == "zip").unwrap_or(false) {
                                target_path
                            } else {
                                target_path.with_extension("zip")
                            };

                            toast_c.add_toast(Toast::new("Compactando e exportando projeto para ZIP..."));

                            let (sender, receiver) = async_channel::bounded(1);
                            let proj_dir_thread = proj_dir.clone();
                            let target_thread = final_path.clone();

                            std::thread::spawn(move || {
                                let res = ZipService::export_project(&proj_dir_thread, &target_thread);
                                let _ = sender.send_blocking(res);
                            });

                            let toast_done = toast_c.clone();
                            glib::spawn_future_local(async move {
                                if let Ok(res) = receiver.recv().await {
                                    match res {
                                        Ok(_) => {
                                            toast_done.add_toast(Toast::new(&format!(
                                                "Projeto exportado com sucesso: {}",
                                                final_path.file_name().unwrap_or_default().to_string_lossy()
                                            )));
                                        }
                                        Err(e) => {
                                            toast_done.add_toast(Toast::new(&format!("Erro ao exportar ZIP: {}", e)));
                                        }
                                    }
                                }
                            });
                        }
                    }
                }
            });

            chooser.show();
        });

        let exp_zip_click = export_zip_action.clone();
        btn_export_zip.connect_clicked(move |_| {
            exp_zip_click();
        });

        // Global Window Keyboard Controller (Ctrl+O, Ctrl+S, Ctrl+B, Ctrl+Z, Ctrl+Y, Ctrl+J, Ctrl+Shift+S, Ctrl+Shift+E)
        let key_controller = EventControllerKey::new();
        let open_key = open_action.clone();
        let btn_save_key = btn_save.clone();
        let btn_compile_key = btn_compile.clone();
        let btn_undo_key = btn_undo.clone();
        let btn_redo_key = btn_redo.clone();
        let btn_toggle_key = btn_toggle_logs.clone();
        let btn_spell_key = btn_spell_status.clone();
        let exp_pdf_key = export_pdf_action.clone();
        let exp_zip_key = export_zip_action.clone();

        key_controller.connect_key_pressed(move |_ctrl, keyval, _keycode, modifier| {
            let is_ctrl = modifier.contains(gtk4::gdk::ModifierType::CONTROL_MASK);
            let is_shift = modifier.contains(gtk4::gdk::ModifierType::SHIFT_MASK);

            if is_ctrl && is_shift {
                match keyval {
                    Key::s | Key::S => {
                        exp_pdf_key();
                        glib::Propagation::Stop
                    }
                    Key::e | Key::E => {
                        exp_zip_key();
                        glib::Propagation::Stop
                    }
                    _ => glib::Propagation::Proceed,
                }
            } else if is_ctrl {
                match keyval {
                    Key::o | Key::O => {
                        open_key();
                        glib::Propagation::Stop
                    }
                    Key::s | Key::S => {
                        btn_save_key.emit_clicked();
                        glib::Propagation::Stop
                    }
                    Key::b | Key::B => {
                        btn_compile_key.emit_clicked();
                        glib::Propagation::Stop
                    }
                    Key::z | Key::Z => {
                        btn_undo_key.emit_clicked();
                        glib::Propagation::Stop
                    }
                    Key::y | Key::Y => {
                        btn_redo_key.emit_clicked();
                        glib::Propagation::Stop
                    }
                    Key::j | Key::J => {
                        btn_toggle_key.set_active(!btn_toggle_key.is_active());
                        glib::Propagation::Stop
                    }
                    _ => glib::Propagation::Proceed,
                }
            } else if keyval == Key::F7 {
                btn_spell_key.emit_clicked();
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        self.window.add_controller(key_controller);

        // --- 5. Compilation Pipeline (Manual + Auto-compile with Debounce & Completeness Check) ---
        let state_comp = self.state.clone();
        let pdf_comp = self.pdf_viewer.clone();
        let spinner_comp = self.spinner.clone();
        let status_comp = self.status_label.clone();
        let log_comp = self.log_view.clone();
        let toast_comp = self.toast_overlay.clone();
        let btn_toggle_comp = btn_toggle_logs.clone();

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
            status_comp.set_text("Compilando documento LaTeX...");

            let state_done = state_comp.clone();
            let spinner_done = spinner_comp.clone();
            let status_done = status_comp.clone();
            let log_done = log_comp.clone();
            let pdf_done = pdf_comp.clone();
            let toast_done = toast_comp.clone();
            let btn_toggle_done = btn_toggle_comp.clone();
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
                            log_done.buffer().set_text(&summary.raw_log);

                            if summary.success {
                                toast_done.add_toast(Toast::new("Compilação bem-sucedida!"));
                                let cfg = ProjectConfig::new("VibeTeX Project", "main.tex");
                                let pdf_path = CompilerRunner::get_pdf_path(&proj_dir_done, &cfg);
                                if pdf_path.exists() {
                                    pdf_done.load_pdf(&pdf_path);
                                }
                            } else {
                                toast_done.add_toast(Toast::new("Houve erros durante a compilação."));
                                // Automatically open debug drawer when errors occur
                                btn_toggle_done.set_active(true);
                            }
                        }
                        Err(e) => {
                            let msg = format!("Falha ao executar compilador: {}", e);
                            status_done.set_text(&msg);
                            log_done.buffer().set_text(&msg);
                            toast_done.add_toast(Toast::new(&msg));
                            btn_toggle_done.set_active(true);
                        }
                    }
                }
            });
        });

        // Debounce timer with generation counter: zero risk of GLib SourceId panics
        let compile_generation: Rc<Cell<u64>> = Rc::new(Cell::new(0));
        let btn_auto_ref = self.btn_auto_compile.clone();
        let state_debounce = self.state.clone();
        let code_debounce = self.code_editor.clone();
        let status_debounce = self.status_label.clone();

        let trigger_auto_compile = {
            let tc = trigger_compile.clone();
            let gen_counter = compile_generation.clone();
            let auto_btn = btn_auto_ref.clone();
            let st_check = state_debounce.clone();
            let code_check = code_debounce.clone();
            let status_check = status_debounce.clone();

            Rc::new(move || {
                if !auto_btn.is_active() {
                    return;
                }
                let current_gen = gen_counter.get().wrapping_add(1);
                gen_counter.set(current_gen);

                let tc_run = tc.clone();
                let gen_check = gen_counter.clone();
                let st_run = st_check.clone();
                let code_run = code_check.clone();
                let status_run = status_check.clone();

                // 700ms debounce
                glib::timeout_add_local_once(Duration::from_millis(700), move || {
                    if gen_check.get() != current_gen {
                        return;
                    }

                    // Check if LaTeX command, macro, or math block is incomplete
                    let (source, cursor) = {
                        let st = st_run.borrow();
                        (st.document.source().to_string(), code_run.get_cursor_offset())
                    };

                    if is_latex_command_incomplete(&source, Some(cursor)) {
                        status_run.set_text("Aguardando conclusão do comando LaTeX... (compilação em pausa)");
                        return;
                    }

                    tc_run();
                });
            })
        };

        // Manual compile button
        let tc_manual = trigger_compile.clone();
        btn_compile.connect_clicked(move |_| {
            tc_manual();
        });

        // --- 6. Code Editor -> Autocomplete, Document Model & Auto-Compile ---
        let state_code = self.state.clone();
        let code_ref = self.code_editor.clone();
        let auto_comp_code = trigger_auto_compile.clone();

        self.code_editor.buffer().connect_changed(move |buffer| {
            if code_ref.is_internal_update() {
                return;
            }

            // 1. Live autocomplete suggestions popover
            code_ref.check_and_show_completions();

            // 2. Synchronize buffer text to document model
            let (start, end) = buffer.bounds();
            let text = buffer.text(&start, &end, false).to_string();

            let mut st = state_code.borrow_mut();
            let full_span = vibetex_core::Span::new(0, st.document.source().len());
            let delta = TextDelta::new(full_span, text.clone(), ChangeOrigin::CodeEditor);
            let _ = st.document.apply_delta(delta);

            // 3. Schedule auto-compilation with debounce and completeness check
            auto_comp_code();

            // 4. Schedule live spell-check with debounce
            code_ref.schedule_spell_check();
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
        let log_clear = self.log_view.clone();
        let status_clear = self.status_label.clone();
        btn_clear_logs.connect_clicked(move |_| {
            log_clear.buffer().set_text("");
            status_clear.set_text("Logs limpos.");
        });



        // 12. New Project from Template Dialog
        let toast_tmpl = self.toast_overlay.clone();
        let state_tmpl = self.state.clone();
        let code_tmpl = self.code_editor.clone();
        let tc_tmpl = trigger_compile.clone();
        let files_tmpl = self.files_list.clone();
        let outline_tmpl = self.outline_list.clone();
        let bib_tmpl = self.bib_list.clone();
        let collapsed_tmpl = self.collapsed_dirs.clone();

        btn_new_proj.connect_clicked(move |_| {
            if let Some(template) = TemplateCatalog::get("abnt") {
                let st = state_tmpl.borrow();
                let _ = template.instantiate(&st.project_dir);
                let main_path = st.project_dir.join("main.tex");
                if let Ok(content) = std::fs::read_to_string(&main_path) {
                    code_tmpl.set_text_silently(&content);
                    toast_tmpl.add_toast(Toast::new("Modelo ABNT aplicado com sucesso!"));
                    collapsed_tmpl.borrow_mut().clear();
                    Self::refresh_file_tree_impl(
                        &files_tmpl,
                        &state_tmpl,
                        &code_tmpl,
                        &outline_tmpl,
                        Some(&toast_tmpl),
                        &collapsed_tmpl,
                    );
                    Self::refresh_outline_impl(&outline_tmpl, &state_tmpl, &code_tmpl);
                    Self::refresh_bib_list_impl(&bib_tmpl, &state_tmpl, &code_tmpl);
                    tc_tmpl();
                }
            }
        });
    }
}
