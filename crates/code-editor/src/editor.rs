use crate::completion::{BibSuggestion, CompletionEngine, CompletionItem};
use gtk4::gdk::Key;
use gtk4::pango::EllipsizeMode;
use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, EventControllerKey, GestureClick, Image, Label, ListBox, ListBoxRow,
    Orientation, Popover, ScrolledWindow, Separator,
};
use sourceview5::prelude::*;
use sourceview5::{Buffer, LanguageManager, StyleSchemeManager, View};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;
use vibetex_spellcheck::{LanguageMode, SpellChecker, SpellingIssue};

pub struct CodeEditorWidget {
    scrolled: ScrolledWindow,
    view: View,
    buffer: Buffer,
    popover: Popover,
    listbox: ListBox,
    current_items: Rc<RefCell<Vec<CompletionItem>>>,
    bib_suggestions: Rc<RefCell<Vec<BibSuggestion>>>,
    labels: Rc<RefCell<Vec<String>>>,
    is_internal_update: Rc<RefCell<bool>>,
    spell_checker: Rc<SpellChecker>,
    language_mode: Rc<RefCell<LanguageMode>>,
    current_spell_issues: Rc<RefCell<Vec<SpellingIssue>>>,
    #[allow(dead_code)]
    spell_popover: Popover,
    spell_debounce_id: Rc<RefCell<u64>>,
    on_spell_checked: Rc<RefCell<Option<Box<dyn Fn(usize, LanguageMode)>>>>,
}

impl CodeEditorWidget {
    pub fn new() -> Self {
        let lm = LanguageManager::default();
        let latex_lang = lm.language("latex");

        let buffer = Buffer::new(None);
        if let Some(ref lang) = latex_lang {
            buffer.set_language(Some(lang));
        }

        // Apply dark/light style scheme if available
        let sm = StyleSchemeManager::default();
        if let Some(scheme) = sm.scheme("Adwaita-dark").or_else(|| sm.scheme("classic")) {
            buffer.set_style_scheme(Some(&scheme));
        }

        let view = View::with_buffer(&buffer);
        view.set_show_line_numbers(true);
        view.set_highlight_current_line(true);
        view.set_auto_indent(true);
        view.set_indent_on_tab(true);
        view.set_indent_width(2);
        view.set_tab_width(2);
        view.set_monospace(true);
        view.set_insert_spaces_instead_of_tabs(true);

        let scrolled = ScrolledWindow::new();
        scrolled.set_vexpand(true);
        scrolled.set_hexpand(true);
        scrolled.set_child(Some(&view));

        // Autocomplete popover
        let popover = Popover::builder()
            .autohide(false)
            .has_arrow(false)
            .build();
        popover.set_parent(&view);
        popover.add_css_class("menu");

        let listbox = ListBox::new();
        listbox.add_css_class("navigation-sidebar");
        listbox.set_selection_mode(gtk4::SelectionMode::Single);

        let pop_scroll = ScrolledWindow::new();
        pop_scroll.set_max_content_height(240);
        pop_scroll.set_propagate_natural_height(true);
        pop_scroll.set_min_content_width(320);
        pop_scroll.set_child(Some(&listbox));
        popover.set_child(Some(&pop_scroll));

        let current_items: Rc<RefCell<Vec<CompletionItem>>> = Rc::new(RefCell::new(Vec::new()));
        let bib_suggestions: Rc<RefCell<Vec<BibSuggestion>>> = Rc::new(RefCell::new(Vec::new()));
        let labels: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let is_internal_update = Rc::new(RefCell::new(false));

        // Key controller on View for completion navigation
        let key_controller = EventControllerKey::new();
        let pop_ref = popover.clone();
        let list_ref = listbox.clone();
        let items_ref = current_items.clone();
        let buffer_ref = buffer.clone();
        let view_ref = view.clone();
        let internal_ref = is_internal_update.clone();

        key_controller.connect_key_pressed(move |_, keyval, _, _| {
            if pop_ref.is_visible() {
                match keyval {
                    Key::Down => {
                        let total = items_ref.borrow().len() as i32;
                        if total > 0 {
                            let current_idx = list_ref.selected_row().map(|r| r.index()).unwrap_or(-1);
                            let next_idx = (current_idx + 1).min(total - 1);
                            if let Some(row) = list_ref.row_at_index(next_idx) {
                                list_ref.select_row(Some(&row));
                            }
                        }
                        return glib::Propagation::Stop;
                    }
                    Key::Up => {
                        let total = items_ref.borrow().len() as i32;
                        if total > 0 {
                            let current_idx = list_ref.selected_row().map(|r| r.index()).unwrap_or(0);
                            let prev_idx = (current_idx - 1).max(0);
                            if let Some(row) = list_ref.row_at_index(prev_idx) {
                                list_ref.select_row(Some(&row));
                            }
                        }
                        return glib::Propagation::Stop;
                    }
                    Key::Return | Key::KP_Enter | Key::Tab => {
                        if let Some(row) = list_ref.selected_row() {
                            let idx = row.index() as usize;
                            let items = items_ref.borrow();
                            if let Some(item) = items.get(idx) {
                                Self::apply_completion_to_buffer(
                                    &buffer_ref,
                                    &view_ref,
                                    &internal_ref,
                                    item,
                                );
                                pop_ref.popdown();
                                return glib::Propagation::Stop;
                            }
                        }
                    }
                    Key::Escape => {
                        pop_ref.popdown();
                        return glib::Propagation::Stop;
                    }
                    _ => {}
                }
            }
            glib::Propagation::Proceed
        });

        view.add_controller(key_controller);

        // Click row activation in autocomplete popover
        let pop_click = popover.clone();
        let items_click = current_items.clone();
        let buf_click = buffer.clone();
        let view_click = view.clone();
        let internal_click = is_internal_update.clone();

        listbox.connect_row_activated(move |_, row| {
            let idx = row.index() as usize;
            let items = items_click.borrow();
            if let Some(item) = items.get(idx) {
                Self::apply_completion_to_buffer(
                    &buf_click,
                    &view_click,
                    &internal_click,
                    item,
                );
                pop_click.popdown();
            }
        });

        let spell_checker = Rc::new(SpellChecker::new());
        let language_mode = Rc::new(RefCell::new(LanguageMode::Portuguese));
        let current_spell_issues = Rc::new(RefCell::new(Vec::<SpellingIssue>::new()));
        let spell_debounce_id = Rc::new(RefCell::new(0u64));
        let on_spell_checked = Rc::new(RefCell::new(None));

        let spell_tag = gtk4::TextTag::builder()
            .name("spell-error")
            .underline(gtk4::pango::Underline::Error)
            .build();
        buffer.tag_table().add(&spell_tag);

        let spell_popover = Popover::builder()
            .autohide(true)
            .has_arrow(true)
            .build();
        spell_popover.set_parent(&view);

        // Right-click gesture for spell correction popover
        let right_click = GestureClick::new();
        right_click.set_button(3);
        right_click.set_propagation_phase(gtk4::PropagationPhase::Capture);

        let sp_checker = spell_checker.clone();
        let sp_lang = language_mode.clone();
        let sp_issues = current_spell_issues.clone();
        let sp_pop = spell_popover.clone();
        let sp_view = view.clone();
        let sp_buf = buffer.clone();
        let sp_internal = is_internal_update.clone();
        let sp_on_checked = on_spell_checked.clone();

        right_click.connect_pressed(move |gesture, n_press, x, y| {
            if n_press != 1 {
                return;
            }

            let (bx, by) = sp_view.window_to_buffer_coords(gtk4::TextWindowType::Widget, x as i32, y as i32);
            if let Some(iter) = sp_view.iter_at_location(bx, by) {
                let offset = iter.offset() as usize;
                let issues = sp_issues.borrow();
                let clicked_issue = issues
                    .iter()
                    .find(|iss| offset >= iss.char_start && offset <= iss.char_end)
                    .cloned();

                drop(issues);

                if let Some(issue) = clicked_issue {
                    let lang = *sp_lang.borrow();
                    if lang == LanguageMode::Disabled {
                        return;
                    }

                    gesture.set_state(gtk4::EventSequenceState::Claimed);

                    let pop_box = GtkBox::new(Orientation::Vertical, 4);
                    pop_box.set_margin_start(10);
                    pop_box.set_margin_end(10);
                    pop_box.set_margin_top(8);
                    pop_box.set_margin_bottom(8);

                    // Header with misspelled word & language badge
                    let header_box = GtkBox::new(Orientation::Horizontal, 6);
                    let title_icon = Image::from_icon_name("tools-check-spelling-symbolic");
                    let title_label = Label::new(None);
                    title_label.set_markup(&format!("<b>{}</b>", issue.word));
                    let lang_badge = Label::new(Some(lang.short_label()));
                    lang_badge.add_css_class("dim-label");

                    header_box.append(&title_icon);
                    header_box.append(&title_label);
                    header_box.append(&lang_badge);
                    pop_box.append(&header_box);
                    pop_box.append(&Separator::new(Orientation::Horizontal));

                    // Suggestions list
                    let suggestions = sp_checker.suggest(&issue.word, lang, 5);
                    if suggestions.is_empty() {
                        let none_lbl = Label::new(Some("Nenhuma sugestão encontrada"));
                        none_lbl.add_css_class("dim-label");
                        pop_box.append(&none_lbl);
                    } else {
                        for sug in suggestions {
                            let btn = Button::with_label(&sug);
                            btn.add_css_class("flat");

                            let buf_c = sp_buf.clone();
                            let pop_c = sp_pop.clone();
                            let int_c = sp_internal.clone();
                            let chk_c = sp_checker.clone();
                            let lang_c = sp_lang.clone();
                            let iss_c = sp_issues.clone();
                            let issue_start = issue.char_start;
                            let issue_end = issue.char_end;
                            let sug_clone = sug.clone();

                            let on_c = sp_on_checked.clone();

                            btn.connect_clicked(move |_| {
                                *int_c.borrow_mut() = true;
                                let mut start_iter = buf_c.iter_at_offset(issue_start as i32);
                                let mut end_iter = buf_c.iter_at_offset(issue_end as i32);
                                buf_c.delete(&mut start_iter, &mut end_iter);
                                buf_c.insert(&mut start_iter, &sug_clone);
                                *int_c.borrow_mut() = false;

                                pop_c.popdown();

                                Self::recheck_spelling_impl(&buf_c, &chk_c, *lang_c.borrow(), &iss_c, &on_c);
                            });

                            pop_box.append(&btn);
                        }
                    }

                    pop_box.append(&Separator::new(Orientation::Horizontal));

                    // Add to personal dictionary action
                    let btn_add = Button::builder()
                        .label("Adicionar ao dicionário")
                        .icon_name("list-add-symbolic")
                        .css_classes(["flat"])
                        .build();
                    {
                        let chk_c = sp_checker.clone();
                        let pop_c = sp_pop.clone();
                        let buf_c = sp_buf.clone();
                        let lang_c = sp_lang.clone();
                        let iss_c = sp_issues.clone();
                        let on_c = sp_on_checked.clone();
                        let word_to_add = issue.word.clone();

                        btn_add.connect_clicked(move |_| {
                            chk_c.add_to_personal_dict(&word_to_add);
                            pop_c.popdown();
                            Self::recheck_spelling_impl(&buf_c, &chk_c, *lang_c.borrow(), &iss_c, &on_c);
                        });
                    }
                    pop_box.append(&btn_add);

                    // Ignore word action
                    let btn_ignore = Button::builder()
                        .label("Ignorar palavra")
                        .icon_name("edit-clear-symbolic")
                        .css_classes(["flat"])
                        .build();
                    {
                        let chk_c = sp_checker.clone();
                        let pop_c = sp_pop.clone();
                        let buf_c = sp_buf.clone();
                        let lang_c = sp_lang.clone();
                        let iss_c = sp_issues.clone();
                        let on_c = sp_on_checked.clone();
                        let word_to_ignore = issue.word.clone();

                        btn_ignore.connect_clicked(move |_| {
                            chk_c.ignore_word(&word_to_ignore);
                            pop_c.popdown();
                            Self::recheck_spelling_impl(&buf_c, &chk_c, *lang_c.borrow(), &iss_c, &on_c);
                        });
                    }
                    pop_box.append(&btn_ignore);

                    sp_pop.set_child(Some(&pop_box));

                    let point_rect = gtk4::gdk::Rectangle::new(x as i32, y as i32, 1, 1);
                    sp_pop.set_pointing_to(Some(&point_rect));
                    sp_pop.popup();
                }
            }
        });

        view.add_controller(right_click);

        Self {
            scrolled,
            view,
            buffer,
            popover,
            listbox,
            current_items,
            bib_suggestions,
            labels,
            is_internal_update,
            spell_checker,
            language_mode,
            current_spell_issues,
            spell_popover,
            spell_debounce_id,
            on_spell_checked,
        }
    }

    pub fn widget(&self) -> &ScrolledWindow {
        &self.scrolled
    }

    pub fn view(&self) -> &View {
        &self.view
    }

    pub fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    pub fn get_text(&self) -> String {
        let (start, end) = self.buffer.bounds();
        self.buffer.text(&start, &end, false).to_string()
    }

    /// Sets buffer text with internal update flag active to prevent circular events.
    pub fn set_text_silently(&self, text: &str) {
        *self.is_internal_update.borrow_mut() = true;
        self.buffer.set_text(text);
        *self.is_internal_update.borrow_mut() = false;
        self.recheck_spelling();
    }

    pub fn is_internal_update(&self) -> bool {
        *self.is_internal_update.borrow()
    }

    pub fn get_cursor_offset(&self) -> usize {
        if let Some(mark) = self.buffer.mark("insert") {
            let iter = self.buffer.iter_at_mark(&mark);
            iter.offset() as usize
        } else {
            0
        }
    }

    pub fn set_cursor_offset(&self, offset: usize) {
        let mut iter = self.buffer.iter_at_offset(offset as i32);
        self.buffer.place_cursor(&iter);
        self.view.scroll_to_iter(&mut iter, 0.1, false, 0.0, 0.0);
    }

    pub fn set_bib_suggestions(&self, bibs: Vec<BibSuggestion>) {
        *self.bib_suggestions.borrow_mut() = bibs;
    }

    pub fn set_labels(&self, labels: Vec<String>) {
        *self.labels.borrow_mut() = labels;
    }

    // ------------------ Spell Checker Public API ------------------

    pub fn spell_checker(&self) -> &Rc<SpellChecker> {
        &self.spell_checker
    }

    pub fn language_mode(&self) -> LanguageMode {
        *self.language_mode.borrow()
    }

    pub fn set_language_mode(&self, mode: LanguageMode) -> usize {
        *self.language_mode.borrow_mut() = mode;
        self.recheck_spelling()
    }

    pub fn connect_spell_checked<F: Fn(usize, LanguageMode) + 'static>(&self, f: F) {
        *self.on_spell_checked.borrow_mut() = Some(Box::new(f));
    }

    pub fn recheck_spelling(&self) -> usize {
        Self::recheck_spelling_impl(
            &self.buffer,
            &self.spell_checker,
            *self.language_mode.borrow(),
            &self.current_spell_issues,
            &self.on_spell_checked,
        )
    }

    pub fn schedule_spell_check(self: &Rc<Self>) {
        let deb_id = self.spell_debounce_id.clone();
        let next_id = {
            let mut cur = deb_id.borrow_mut();
            *cur += 1;
            *cur
        };

        let self_clone = self.clone();
        glib::timeout_add_local_once(Duration::from_millis(450), move || {
            if *deb_id.borrow() == next_id {
                self_clone.recheck_spelling();
            }
        });
    }

    pub fn current_spell_issues_count(&self) -> usize {
        self.current_spell_issues.borrow().len()
    }

    pub fn jump_to_next_spell_error(&self) -> Option<usize> {
        let issues = self.current_spell_issues.borrow();
        if issues.is_empty() {
            return None;
        }

        let cur_offset = self.get_cursor_offset();
        let next_issue = issues
            .iter()
            .find(|iss| iss.char_start > cur_offset)
            .or_else(|| issues.first());

        if let Some(issue) = next_issue {
            self.set_cursor_offset(issue.char_start);
            Some(issue.char_start)
        } else {
            None
        }
    }

    fn recheck_spelling_impl(
        buffer: &Buffer,
        checker: &SpellChecker,
        mode: LanguageMode,
        issues_store: &Rc<RefCell<Vec<SpellingIssue>>>,
        on_checked: &Rc<RefCell<Option<Box<dyn Fn(usize, LanguageMode)>>>>,
    ) -> usize {
        let (start, end) = buffer.bounds();
        let text = buffer.text(&start, &end, false).to_string();
        let issues = checker.check_latex(&text, mode);

        buffer.remove_tag_by_name("spell-error", &start, &end);

        if mode != LanguageMode::Disabled {
            for issue in &issues {
                let start_iter = buffer.iter_at_offset(issue.char_start as i32);
                let end_iter = buffer.iter_at_offset(issue.char_end as i32);
                buffer.apply_tag_by_name("spell-error", &start_iter, &end_iter);
            }
        }

        let count = issues.len();
        *issues_store.borrow_mut() = issues;

        if let Some(ref cb) = *on_checked.borrow() {
            cb(count, mode);
        }

        count
    }

    // ------------------ Autocomplete API ------------------

    /// Inspects text before the cursor, queries suggestions, and presents the popover.
    pub fn check_and_show_completions(&self) {
        if *self.is_internal_update.borrow() {
            return;
        }

        let cursor_offset = self.get_cursor_offset();
        let text = self.get_text();
        if cursor_offset > text.len() {
            self.popover.popdown();
            return;
        }

        let before_cursor = &text[..cursor_offset];
        let bibs = self.bib_suggestions.borrow();
        let labels = self.labels.borrow();

        let items = CompletionEngine::query(before_cursor, &bibs, &labels);

        if items.is_empty() {
            self.popover.popdown();
            *self.current_items.borrow_mut() = Vec::new();
            return;
        }

        // Populate listbox
        while let Some(child) = self.listbox.first_child() {
            self.listbox.remove(&child);
        }

        for item in &items {
            let row = ListBoxRow::new();
            let row_box = GtkBox::new(Orientation::Horizontal, 8);
            row_box.set_margin_start(8);
            row_box.set_margin_end(8);
            row_box.set_margin_top(4);
            row_box.set_margin_bottom(4);

            let icon = Image::from_icon_name(item.kind.icon_name());
            row_box.append(&icon);

            let label_widget = Label::new(Some(&item.label));
            label_widget.set_xalign(0.0);
            label_widget.add_css_class("monospace");
            row_box.append(&label_widget);

            let spacer = GtkBox::new(Orientation::Horizontal, 0);
            spacer.set_hexpand(true);
            row_box.append(&spacer);

            let detail_widget = Label::new(Some(&item.detail));
            detail_widget.set_xalign(1.0);
            detail_widget.add_css_class("dim-label");
            detail_widget.set_ellipsize(EllipsizeMode::End);
            detail_widget.set_max_width_chars(30);
            row_box.append(&detail_widget);

            row.set_child(Some(&row_box));
            self.listbox.append(&row);
        }

        *self.current_items.borrow_mut() = items;

        if let Some(first_row) = self.listbox.row_at_index(0) {
            self.listbox.select_row(Some(&first_row));
        }

        // Position popover at cursor
        if let Some(mark) = self.buffer.mark("insert") {
            let iter = self.buffer.iter_at_mark(&mark);
            let rect = self.view.iter_location(&iter);
            let (win_x, win_y) = self.view.buffer_to_window_coords(gtk4::TextWindowType::Widget, rect.x(), rect.y());
            let point_rect = gtk4::gdk::Rectangle::new(win_x, win_y, rect.width().max(1), rect.height().max(18));
            self.popover.set_pointing_to(Some(&point_rect));
            self.popover.popup();
        }
    }

    fn apply_completion_to_buffer(
        buffer: &Buffer,
        view: &View,
        is_internal: &Rc<RefCell<bool>>,
        item: &CompletionItem,
    ) {
        *is_internal.borrow_mut() = true;

        if let Some(mark) = buffer.mark("insert") {
            let mut end_iter = buffer.iter_at_mark(&mark);
            let mut start_iter = end_iter;
            start_iter.backward_chars(item.replace_len as i32);

            buffer.delete(&mut start_iter, &mut end_iter);
            buffer.insert(&mut start_iter, &item.insert_text);

            if item.cursor_rewind > 0 {
                let mut cursor_iter = start_iter;
                cursor_iter.backward_chars(item.cursor_rewind as i32);
                buffer.place_cursor(&cursor_iter);
                view.scroll_to_iter(&mut cursor_iter, 0.1, false, 0.0, 0.0);
            }
        }

        *is_internal.borrow_mut() = false;
    }
}

impl Default for CodeEditorWidget {
    fn default() -> Self {
        Self::new()
    }
}
