use gtk4::prelude::*;
use gtk4::{Box, Button, DrawingArea, Label, Orientation, ScrolledWindow};
use poppler::Document;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

pub struct PdfState {
    pub document: Option<Document>,
    pub current_page: usize,
    pub total_pages: usize,
    pub zoom: f64,
    pub pdf_path: Option<PathBuf>,
}

pub struct PdfViewerWidget {
    container: Box,
    drawing_area: DrawingArea,
    page_label: Label,
    state: Rc<RefCell<PdfState>>,
}

impl PdfViewerWidget {
    pub fn new() -> Self {
        let state = Rc::new(RefCell::new(PdfState {
            document: None,
            current_page: 0,
            total_pages: 0,
            zoom: 1.0,
            pdf_path: None,
        }));

        let container = Box::new(Orientation::Vertical, 0);

        // Toolbar
        let toolbar = Box::new(Orientation::Horizontal, 6);
        toolbar.set_margin_start(8);
        toolbar.set_margin_end(8);
        toolbar.set_margin_top(6);
        toolbar.set_margin_bottom(6);

        let btn_prev = Button::from_icon_name("go-previous-symbolic");
        let btn_next = Button::from_icon_name("go-next-symbolic");
        let page_label = Label::new(Some("Página: - / -"));

        let btn_zoom_out = Button::from_icon_name("zoom-out-symbolic");
        let btn_zoom_in = Button::from_icon_name("zoom-in-symbolic");
        let btn_zoom_fit = Button::from_icon_name("zoom-fit-best-symbolic");

        toolbar.append(&btn_prev);
        toolbar.append(&btn_next);
        toolbar.append(&page_label);

        let spacer = Box::new(Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        toolbar.append(&spacer);

        toolbar.append(&btn_zoom_out);
        toolbar.append(&btn_zoom_in);
        toolbar.append(&btn_zoom_fit);

        container.append(&toolbar);

        // Drawing Area inside ScrolledWindow
        let scrolled = ScrolledWindow::new();
        scrolled.set_vexpand(true);
        scrolled.set_hexpand(true);

        let drawing_area = DrawingArea::new();
        scrolled.set_child(Some(&drawing_area));
        container.append(&scrolled);

        let viewer = Self {
            container,
            drawing_area,
            page_label,
            state,
        };

        viewer.setup_draw_and_actions(btn_prev, btn_next, btn_zoom_in, btn_zoom_out, btn_zoom_fit);
        viewer
    }

    pub fn widget(&self) -> &Box {
        &self.container
    }

    pub fn load_pdf(&self, path: &Path) -> bool {
        let uri = format!("file://{}", path.to_string_lossy());
        match Document::from_file(&uri, None) {
            Ok(doc) => {
                let n_pages = doc.n_pages() as usize;
                {
                    let mut st = self.state.borrow_mut();
                    st.document = Some(doc);
                    st.current_page = 0;
                    st.total_pages = n_pages;
                    st.pdf_path = Some(path.to_path_buf());
                }
                self.update_page_label();
                self.drawing_area.queue_draw();
                true
            }
            Err(_) => false,
        }
    }

    fn update_page_label(&self) {
        let st = self.state.borrow();
        if st.total_pages > 0 {
            self.page_label.set_text(&format!(
                "Página: {} / {} ({:.0}%)",
                st.current_page + 1,
                st.total_pages,
                st.zoom * 100.0
            ));
        } else {
            self.page_label.set_text("Página: - / -");
        }
    }

    fn setup_draw_and_actions(
        &self,
        btn_prev: Button,
        btn_next: Button,
        btn_zoom_in: Button,
        btn_zoom_out: Button,
        btn_zoom_fit: Button,
    ) {
        let state = self.state.clone();
        self.drawing_area.set_draw_func(move |_area, cr, width, height| {
            let st = state.borrow();
            if let Some(ref doc) = st.document {
                if let Some(page) = doc.page(st.current_page as i32) {
                    let (page_w, page_h) = page.size();
                    let zoom = st.zoom;
                    let target_w = page_w * zoom;
                    let target_h = page_h * zoom;

                    // Centering
                    let offset_x = (width as f64 - target_w).max(0.0) / 2.0;
                    let offset_y = (height as f64 - target_h).max(0.0) / 2.0;

                    // Fill background
                    cr.set_source_rgb(0.92, 0.92, 0.94);
                    let _ = cr.paint();

                    // White page surface with drop shadow
                    cr.save().ok();
                    cr.translate(offset_x, offset_y);
                    cr.set_source_rgb(1.0, 1.0, 1.0);
                    cr.rectangle(0.0, 0.0, target_w, target_h);
                    let _ = cr.fill();

                    // Render PDF page
                    cr.scale(zoom, zoom);
                    page.render(cr);
                    cr.restore().ok();
                    return;
                }
            }

            // Fallback screen when no PDF is compiled
            cr.set_source_rgb(0.96, 0.96, 0.97);
            let _ = cr.paint();

            cr.set_source_rgb(0.4, 0.4, 0.45);
            cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
            cr.set_font_size(16.0);
            let text = "Nenhum PDF compilado ainda";
            let ext = cr.text_extents(text).unwrap();
            let x = (width as f64 - ext.width()) / 2.0;
            let y = height as f64 / 2.0 - 10.0;
            cr.move_to(x, y);
            let _ = cr.show_text(text);

            cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
            cr.set_font_size(12.0);
            let sub = "Pressione Compilar (Ctrl+B) para gerar a visualização";
            let ext_sub = cr.text_extents(sub).unwrap();
            cr.move_to((width as f64 - ext_sub.width()) / 2.0, y + 25.0);
            let _ = cr.show_text(sub);
        });

        // Prev page
        let state_prev = self.state.clone();
        let draw_prev = self.drawing_area.clone();
        let label_prev = self.page_label.clone();
        btn_prev.connect_clicked(move |_| {
            let mut st = state_prev.borrow_mut();
            if st.current_page > 0 {
                st.current_page -= 1;
                label_prev.set_text(&format!(
                    "Página: {} / {} ({:.0}%)",
                    st.current_page + 1,
                    st.total_pages,
                    st.zoom * 100.0
                ));
                draw_prev.queue_draw();
            }
        });

        // Next page
        let state_next = self.state.clone();
        let draw_next = self.drawing_area.clone();
        let label_next = self.page_label.clone();
        btn_next.connect_clicked(move |_| {
            let mut st = state_next.borrow_mut();
            if st.total_pages > 0 && st.current_page + 1 < st.total_pages {
                st.current_page += 1;
                label_next.set_text(&format!(
                    "Página: {} / {} ({:.0}%)",
                    st.current_page + 1,
                    st.total_pages,
                    st.zoom * 100.0
                ));
                draw_next.queue_draw();
            }
        });

        // Zoom In
        let state_in = self.state.clone();
        let draw_in = self.drawing_area.clone();
        let label_in = self.page_label.clone();
        btn_zoom_in.connect_clicked(move |_| {
            let mut st = state_in.borrow_mut();
            st.zoom = (st.zoom * 1.2).min(5.0);
            if st.total_pages > 0 {
                label_in.set_text(&format!(
                    "Página: {} / {} ({:.0}%)",
                    st.current_page + 1,
                    st.total_pages,
                    st.zoom * 100.0
                ));
            }
            draw_in.queue_draw();
        });

        // Zoom Out
        let state_out = self.state.clone();
        let draw_out = self.drawing_area.clone();
        let label_out = self.page_label.clone();
        btn_zoom_out.connect_clicked(move |_| {
            let mut st = state_out.borrow_mut();
            st.zoom = (st.zoom / 1.2).max(0.2);
            if st.total_pages > 0 {
                label_out.set_text(&format!(
                    "Página: {} / {} ({:.0}%)",
                    st.current_page + 1,
                    st.total_pages,
                    st.zoom * 100.0
                ));
            }
            draw_out.queue_draw();
        });

        // Zoom Fit
        let state_fit = self.state.clone();
        let draw_fit = self.drawing_area.clone();
        let label_fit = self.page_label.clone();
        btn_zoom_fit.connect_clicked(move |_| {
            let mut st = state_fit.borrow_mut();
            st.zoom = 1.0;
            if st.total_pages > 0 {
                label_fit.set_text(&format!(
                    "Página: {} / {} ({:.0}%)",
                    st.current_page + 1,
                    st.total_pages,
                    st.zoom * 100.0
                ));
            }
            draw_fit.queue_draw();
        });
    }
}

impl Default for PdfViewerWidget {
    fn default() -> Self {
        Self::new()
    }
}
