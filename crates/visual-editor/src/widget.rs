use gtk4::prelude::*;
use gtk4::{Box, Button, Entry, Frame, Label, Orientation, ScrolledWindow, Separator};
use std::cell::RefCell;
use std::rc::Rc;
use vibetex_core::{ChangeOrigin, TextDelta};
use vibetex_document_model::DocumentOperations;
use vibetex_latex_parser::*;

pub type DeltaCallback = Rc<dyn Fn(TextDelta)>;

pub struct VisualEditorWidget {
    container: Box,
    content_box: Box,
    #[allow(dead_code)]
    scrolled: ScrolledWindow,
    on_delta: Rc<RefCell<Option<DeltaCallback>>>,
}

impl VisualEditorWidget {
    pub fn new() -> Self {
        let container = Box::new(Orientation::Vertical, 0);

        // Formatting Action Bar
        let toolbar = Box::new(Orientation::Horizontal, 6);
        toolbar.set_margin_start(12);
        toolbar.set_margin_end(12);
        toolbar.set_margin_top(8);
        toolbar.set_margin_bottom(8);

        let btn_bold = Button::with_label("N");
        btn_bold.set_tooltip_text(Some("Negrito (\\textbf)"));
        let btn_italic = Button::with_label("I");
        btn_italic.set_tooltip_text(Some("Itálico (\\textit)"));

        let btn_sec = Button::with_label("H1");
        btn_sec.set_tooltip_text(Some("Inserir Seção (\\section)"));

        let btn_subsec = Button::with_label("H2");
        btn_subsec.set_tooltip_text(Some("Inserir Subseção (\\subsection)"));

        let btn_math = Button::with_label("∑ fx");
        btn_math.set_tooltip_text(Some("Inserir Bloco de Equação (\\begin{equation})"));

        let btn_cite = Button::with_label("Citar");
        btn_cite.set_tooltip_text(Some("Inserir Citação (\\cite)"));

        let btn_list = Button::with_label("• Lista");
        btn_list.set_tooltip_text(Some("Inserir Lista (\\begin{itemize})"));

        let btn_spacing = Button::with_label("↕ Espaço");
        btn_spacing.set_tooltip_text(Some("Inserir Espaçamento Vertical (\\medskip)"));

        toolbar.append(&btn_bold);
        toolbar.append(&btn_italic);
        toolbar.append(&Separator::new(Orientation::Vertical));
        toolbar.append(&btn_sec);
        toolbar.append(&btn_subsec);
        toolbar.append(&Separator::new(Orientation::Vertical));
        toolbar.append(&btn_math);
        toolbar.append(&btn_cite);
        toolbar.append(&btn_list);
        toolbar.append(&btn_spacing);

        container.append(&toolbar);
        container.append(&Separator::new(Orientation::Horizontal));

        // Scrollable content area
        let scrolled = ScrolledWindow::new();
        scrolled.set_vexpand(true);
        scrolled.set_hexpand(true);

        let content_box = Box::new(Orientation::Vertical, 16);
        content_box.set_margin_start(40);
        content_box.set_margin_end(40);
        content_box.set_margin_top(24);
        content_box.set_margin_bottom(40);

        scrolled.set_child(Some(&content_box));
        container.append(&scrolled);

        let on_delta: Rc<RefCell<Option<DeltaCallback>>> = Rc::new(RefCell::new(None));

        let widget = Self {
            container,
            content_box,
            scrolled,
            on_delta,
        };

        widget.setup_toolbar_actions(
            btn_bold,
            btn_italic,
            btn_sec,
            btn_subsec,
            btn_math,
            btn_cite,
            btn_list,
            btn_spacing,
        );

        widget
    }

    pub fn widget(&self) -> &Box {
        &self.container
    }

    pub fn set_on_delta<F>(&self, callback: F)
    where
        F: Fn(TextDelta) + 'static,
    {
        *self.on_delta.borrow_mut() = Some(Rc::new(callback));
    }

    /// Renders the document AST into rich native GTK4 widgets.
    pub fn render_ast(&self, ast: &DocumentAst, _source: &str) {
        // Clear previous content widgets
        while let Some(child) = self.content_box.first_child() {
            self.content_box.remove(&child);
        }

        // Preamble indicator if present
        if let Some(AstNode::Preamble { ref raw, span: _ }) = ast.preamble {
            let frame = Frame::new(Some("Preâmbulo LaTeX (Preservado)"));
            frame.add_css_class("card");
            frame.set_margin_bottom(8);

            let lbl = Label::new(Some(&format!(
                "{} linhas de configurações, pacotes e definições",
                raw.lines().count()
            )));
            lbl.set_margin_start(12);
            lbl.set_margin_top(8);
            lbl.set_margin_bottom(8);
            lbl.set_xalign(0.0);
            frame.set_child(Some(&lbl));
            self.content_box.append(&frame);
        }

        // Render body nodes
        for node in &ast.nodes {
            match node {
                AstNode::Section { level, title, title_span, span: _ } => {
                    let section_box = Box::new(Orientation::Vertical, 4);

                    let prefix = match level {
                        SectionLevel::Part => "PARTE: ",
                        SectionLevel::Chapter => "CAPÍTULO: ",
                        SectionLevel::Section => "§ ",
                        SectionLevel::Subsection => "§§ ",
                        SectionLevel::Subsubsection => "§§§ ",
                        SectionLevel::Paragraph => "¶ ",
                    };

                    let title_entry = Entry::new();
                    title_entry.set_text(title);
                    title_entry.add_css_class("title-1");

                    let t_span = *title_span;
                    let on_delta_clone = self.on_delta.clone();
                    title_entry.connect_changed(move |entry| {
                        let new_title = entry.text().to_string();
                        let delta = DocumentOperations::update_section_title(
                            t_span,
                            &new_title,
                            ChangeOrigin::VisualEditor,
                        );
                        if let Some(ref cb) = *on_delta_clone.borrow() {
                            cb(delta);
                        }
                    });

                    let label_prefix = Label::new(Some(prefix));
                    label_prefix.set_xalign(0.0);
                    label_prefix.add_css_class("heading");

                    section_box.append(&label_prefix);
                    section_box.append(&title_entry);
                    self.content_box.append(&section_box);
                }
                AstNode::Paragraph { inlines, .. } => {
                    let mut markup = String::new();
                    for inline in inlines {
                        match inline {
                            InlineNode::Text { text, .. } => {
                                markup.push_str(&glib::markup_escape_text(text));
                            }
                            InlineNode::Format { kind, inner, .. } => match kind {
                                FormatKind::Bold => {
                                    markup.push_str(&format!("<b>{}</b>", glib::markup_escape_text(inner)));
                                }
                                FormatKind::Italic => {
                                    markup.push_str(&format!("<i>{}</i>", glib::markup_escape_text(inner)));
                                }
                                FormatKind::Underline => {
                                    markup.push_str(&format!("<u>{}</u>", glib::markup_escape_text(inner)));
                                }
                                FormatKind::Typewriter => {
                                    markup.push_str(&format!("<tt>{}</tt>", glib::markup_escape_text(inner)));
                                }
                                FormatKind::SmallCaps => {
                                    markup.push_str(&format!("<small>{}</small>", glib::markup_escape_text(inner)));
                                }
                            },
                            InlineNode::InlineMath { content, .. } => {
                                markup.push_str(&format!(
                                    "<span foreground='#225588' font_family='serif'><i>${}$</i></span>",
                                    glib::markup_escape_text(content)
                                ));
                            }
                            InlineNode::Citation { key, .. } => {
                                markup.push_str(&format!(
                                    "<span background='#e0e8f5' foreground='#104080'> [{}] </span>",
                                    glib::markup_escape_text(key)
                                ));
                            }
                            InlineNode::Reference { label, .. } => {
                                markup.push_str(&format!(
                                    "<span background='#f5e8e0' foreground='#804010'> [ref: {}] </span>",
                                    glib::markup_escape_text(label)
                                ));
                            }
                            InlineNode::Label { key, .. } => {
                                markup.push_str(&format!(
                                    "<span foreground='#888888'> [label: {}] </span>",
                                    glib::markup_escape_text(key)
                                ));
                            }
                            InlineNode::Protected { raw, .. } => {
                                markup.push_str(&format!(
                                    "<span background='#f0f0f0' foreground='#555555'>[{}]</span>",
                                    glib::markup_escape_text(raw)
                                ));
                            }
                        }
                    }

                    let label = Label::new(None);
                    label.set_markup(&markup);
                    label.set_wrap(true);
                    label.set_xalign(0.0);
                    label.set_selectable(true);
                    self.content_box.append(&label);
                }
                AstNode::MathBlock { content, .. } => {
                    let frame = Frame::new(Some("Equação Matemática (LaTeX)"));
                    frame.add_css_class("card");

                    let math_box = Box::new(Orientation::Vertical, 4);
                    math_box.set_margin_start(16);
                    math_box.set_margin_end(16);
                    math_box.set_margin_top(12);
                    math_box.set_margin_bottom(12);

                    let formula_lbl = Label::new(None);
                    formula_lbl.set_markup(&format!(
                        "<span font_desc='14' font_family='serif' foreground='#1a4c80'><b>{}</b></span>",
                        glib::markup_escape_text(content)
                    ));
                    formula_lbl.set_justify(gtk4::Justification::Center);

                    math_box.append(&formula_lbl);
                    frame.set_child(Some(&math_box));
                    self.content_box.append(&frame);
                }
                AstNode::Itemize { items, .. } | AstNode::Enumerate { items, .. } => {
                    let list_box = Box::new(Orientation::Vertical, 4);
                    list_box.set_margin_start(24);

                    for (idx, item) in items.iter().enumerate() {
                        let bullet = if matches!(node, AstNode::Itemize { .. }) {
                            "• ".to_string()
                        } else {
                            format!("{}. ", idx + 1)
                        };
                        let item_lbl = Label::new(Some(&format!("{}{}", bullet, item.content)));
                        item_lbl.set_xalign(0.0);
                        item_lbl.set_wrap(true);
                        list_box.append(&item_lbl);
                    }
                    self.content_box.append(&list_box);
                }
                AstNode::ProtectedBlock { raw, description, .. } => {
                    let frame = Frame::new(Some(&format!("Bloco Protegido ({})", description)));
                    frame.add_css_class("card");
                    frame.set_margin_top(6);
                    frame.set_margin_bottom(6);

                    let block_box = Box::new(Orientation::Vertical, 6);
                    block_box.set_margin_start(12);
                    block_box.set_margin_end(12);
                    block_box.set_margin_top(8);
                    block_box.set_margin_bottom(8);

                    let snippet = if raw.lines().count() > 3 {
                        format!("{}...", raw.lines().take(3).collect::<Vec<_>>().join("\n"))
                    } else {
                        raw.clone()
                    };

                    let code_lbl = Label::new(Some(&snippet));
                    code_lbl.add_css_class("monospace");
                    code_lbl.set_xalign(0.0);
                    code_lbl.set_wrap(true);
                    block_box.append(&code_lbl);

                    frame.set_child(Some(&block_box));
                    self.content_box.append(&frame);
                }
                AstNode::Comment { text, .. } => {
                    let comment_lbl = Label::new(Some(&format!("% {}", text)));
                    comment_lbl.set_xalign(0.0);
                    comment_lbl.add_css_class("dim-label");
                    self.content_box.append(&comment_lbl);
                }
                AstNode::Spacing { .. } => {
                    let sep = Separator::new(Orientation::Horizontal);
                    sep.set_margin_top(8);
                    sep.set_margin_bottom(8);
                    self.content_box.append(&sep);
                }
                _ => {}
            }
        }
    }

    fn setup_toolbar_actions(
        &self,
        btn_bold: Button,
        btn_italic: Button,
        btn_sec: Button,
        btn_subsec: Button,
        btn_math: Button,
        btn_cite: Button,
        btn_list: Button,
        btn_spacing: Button,
    ) {
        let on_delta_bold = self.on_delta.clone();
        btn_bold.connect_clicked(move |_| {
            let delta = TextDelta::new(
                vibetex_core::Span::empty(0),
                r"\textbf{texto em negrito}".to_string(),
                ChangeOrigin::VisualEditor,
            );
            if let Some(ref cb) = *on_delta_bold.borrow() {
                cb(delta);
            }
        });

        let on_delta_italic = self.on_delta.clone();
        btn_italic.connect_clicked(move |_| {
            let delta = TextDelta::new(
                vibetex_core::Span::empty(0),
                r"\textit{texto em itálico}".to_string(),
                ChangeOrigin::VisualEditor,
            );
            if let Some(ref cb) = *on_delta_italic.borrow() {
                cb(delta);
            }
        });

        let on_delta_sec = self.on_delta.clone();
        btn_sec.connect_clicked(move |_| {
            let delta = DocumentOperations::insert_section(
                0,
                SectionLevel::Section,
                "Nova Seção",
                ChangeOrigin::VisualEditor,
            );
            if let Some(ref cb) = *on_delta_sec.borrow() {
                cb(delta);
            }
        });

        let on_delta_subsec = self.on_delta.clone();
        btn_subsec.connect_clicked(move |_| {
            let delta = DocumentOperations::insert_section(
                0,
                SectionLevel::Subsection,
                "Nova Subseção",
                ChangeOrigin::VisualEditor,
            );
            if let Some(ref cb) = *on_delta_subsec.borrow() {
                cb(delta);
            }
        });

        let on_delta_math = self.on_delta.clone();
        btn_math.connect_clicked(move |_| {
            let delta = DocumentOperations::insert_equation(
                0,
                r"\int_0^\infty e^{-x^2} dx = \frac{\sqrt{\pi}}{2}",
                ChangeOrigin::VisualEditor,
            );
            if let Some(ref cb) = *on_delta_math.borrow() {
                cb(delta);
            }
        });

        let on_delta_cite = self.on_delta.clone();
        btn_cite.connect_clicked(move |_| {
            let delta = DocumentOperations::insert_citation(0, "referencia", ChangeOrigin::VisualEditor);
            if let Some(ref cb) = *on_delta_cite.borrow() {
                cb(delta);
            }
        });

        let on_delta_list = self.on_delta.clone();
        btn_list.connect_clicked(move |_| {
            let delta = DocumentOperations::insert_list(0, false, ChangeOrigin::VisualEditor);
            if let Some(ref cb) = *on_delta_list.borrow() {
                cb(delta);
            }
        });

        let on_delta_spacing = self.on_delta.clone();
        btn_spacing.connect_clicked(move |_| {
            let delta = DocumentOperations::insert_spacing(
                0,
                SpacingKind::MedSkip,
                None,
                ChangeOrigin::VisualEditor,
            );
            if let Some(ref cb) = *on_delta_spacing.borrow() {
                cb(delta);
            }
        });
    }
}

impl Default for VisualEditorWidget {
    fn default() -> Self {
        Self::new()
    }
}
