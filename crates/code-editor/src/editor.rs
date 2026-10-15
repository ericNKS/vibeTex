use crate::completion::{BibSuggestion, CompletionEngine, CompletionItem};
use gtk4::gdk::Key;
use gtk4::pango::EllipsizeMode;
use gtk4::prelude::*;
use gtk4::{Box as GtkBox, EventControllerKey, Image, Label, ListBox, ListBoxRow, Orientation, Popover, ScrolledWindow};
use sourceview5::prelude::*;
use sourceview5::{Buffer, LanguageManager, StyleSchemeManager, View};
use std::cell::RefCell;
use std::rc::Rc;

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
