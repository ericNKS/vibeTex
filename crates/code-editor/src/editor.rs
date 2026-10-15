use gtk4::prelude::*;
use gtk4::ScrolledWindow;
use sourceview5::prelude::*;
use sourceview5::{Buffer, LanguageManager, StyleSchemeManager, View};
use std::cell::RefCell;
use std::rc::Rc;

pub struct CodeEditorWidget {
    scrolled: ScrolledWindow,
    view: View,
    buffer: Buffer,
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

        Self {
            scrolled,
            view,
            buffer,
            is_internal_update: Rc::new(RefCell::new(false)),
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
}

impl Default for CodeEditorWidget {
    fn default() -> Self {
        Self::new()
    }
}
