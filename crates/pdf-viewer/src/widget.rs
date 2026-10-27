use gtk4::prelude::*;
use gtk4::{
    Box, Button, DrawingArea, EventControllerKey, EventControllerMotion, GestureClick, GestureDrag,
    Label, Orientation, Popover, ScrolledWindow, Separator, ToggleButton,
};
use poppler::{ffi, Document, Rectangle, SelectionStyle};
use glib::translate::ToGlibPtr;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageViewMode {
    ContinuousVertical,
    SinglePage,
}

#[derive(Clone, Debug)]
pub enum PdfLinkAction {
    Goto {
        page_index: usize,
        top_offset: f64,
    },
    Uri(String),
}

#[derive(Clone, Debug)]
pub struct PdfLinkItem {
    pub page_index: usize,
    pub rect: (f64, f64, f64, f64), // (x1, y1, x2, y2) in points from top-left of page
    pub action: PdfLinkAction,
    pub tooltip: String,
}

#[derive(Clone, Debug, Default)]
pub struct PageHighlight {
    pub page_index: usize,
    pub rects: Vec<(f64, f64, f64, f64)>, // (x, y, w, h) in page points
}

#[derive(Clone, Debug, Default)]
pub struct PdfSelection {
    pub start_page: usize,
    pub start_pt: (f64, f64),
    pub end_page: usize,
    pub end_pt: (f64, f64),
    pub highlights: Vec<PageHighlight>,
    pub text: String,
}

pub struct PdfState {
    pub document: Option<Document>,
    pub current_page: usize,
    pub total_pages: usize,
    pub zoom: f64,
    pub pdf_path: Option<PathBuf>,
    pub view_mode: PageViewMode,
    pub links: Vec<PdfLinkItem>,
    /// Cached page sizes in points (avoids poppler lookups on every frame).
    pub page_sizes: Vec<(f64, f64)>,
    /// Rendered page bitmaps keyed by page index (valid for `cache_zoom`/`cache_scale`).
    pub page_cache: HashMap<usize, cairo::ImageSurface>,
    pub cache_zoom: f64,
    pub cache_scale: i32,
    /// Incremented on every new document so stale prewarm jobs stop.
    pub doc_generation: u64,
    pub selection: Option<PdfSelection>,
}

const MAX_CACHED_PAGES: usize = 40;
const PREWARM_PAGES: usize = 12;

fn render_page_surface(page: &poppler::Page, zoom: f64, scale: f64) -> Option<cairo::ImageSurface> {
    let (pw, ph) = page.size();
    let w = (pw * zoom * scale).ceil().max(1.0) as i32;
    let h = (ph * zoom * scale).ceil().max(1.0) as i32;
    let surf = cairo::ImageSurface::create(cairo::Format::Rgb24, w, h).ok()?;
    {
        let cr = cairo::Context::new(&surf).ok()?;
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.paint().ok()?;
        cr.scale(zoom * scale, zoom * scale);
        page.render(&cr);
    }
    surf.flush();
    Some(surf)
}

impl PdfState {
    /// Drops the bitmap cache if zoom or HiDPI scale changed.
    fn validate_cache(&mut self, scale: i32) {
        if (self.cache_zoom - self.zoom).abs() > f64::EPSILON || self.cache_scale != scale {
            self.page_cache.clear();
            self.cache_zoom = self.zoom;
            self.cache_scale = scale;
        }
    }

    /// Returns a cached render of the page, rendering it on demand.
    fn page_surface(&mut self, idx: usize, scale: i32) -> Option<cairo::ImageSurface> {
        self.validate_cache(scale);
        if let Some(s) = self.page_cache.get(&idx) {
            return Some(s.clone());
        }
        let doc = self.document.as_ref()?;
        let page = doc.page(idx as i32)?;
        let surf = render_page_surface(&page, self.zoom, scale as f64)?;
        if self.page_cache.len() >= MAX_CACHED_PAGES {
            // Evict the page farthest from the current one.
            let cur = self.current_page as i64;
            if let Some(&far) = self
                .page_cache
                .keys()
                .max_by_key(|k| (**k as i64 - cur).abs())
            {
                self.page_cache.remove(&far);
            }
        }
        self.page_cache.insert(idx, surf.clone());
        Some(surf)
    }
}

type ToastCallback = std::boxed::Box<dyn Fn(&str)>;
type ExportCallback = std::boxed::Box<dyn Fn()>;

pub struct PdfViewerWidget {
    container: Box,
    drawing_area: DrawingArea,
    scrolled: ScrolledWindow,
    page_label: Label,
    btn_zoom_menu: Button,
    btn_mode_vertical: ToggleButton,
    btn_mode_single: ToggleButton,
    btn_copy: Button,
    _btn_export_pdf: Button,
    state: Rc<RefCell<PdfState>>,
    on_toast: Rc<RefCell<Option<ToastCallback>>>,
    on_export_pdf: Rc<RefCell<Option<ExportCallback>>>,
}

impl PdfViewerWidget {
    pub fn new() -> Self {
        let state = Rc::new(RefCell::new(PdfState {
            document: None,
            current_page: 0,
            total_pages: 0,
            zoom: 1.0,
            pdf_path: None,
            view_mode: PageViewMode::ContinuousVertical, // Default to vertical continuous
            links: Vec::new(),
            page_sizes: Vec::new(),
            page_cache: HashMap::new(),
            cache_zoom: 1.0,
            cache_scale: 1,
            doc_generation: 0,
            selection: None,
        }));

        let container = Box::new(Orientation::Vertical, 0);

        // 1. PDF Canvas inside ScrolledWindow (occupies all top space)
        let scrolled = ScrolledWindow::new();
        scrolled.set_vexpand(true);
        scrolled.set_hexpand(true);

        let drawing_area = DrawingArea::new();
        drawing_area.set_focusable(true);
        scrolled.set_child(Some(&drawing_area));
        container.append(&scrolled);

        // Separator between canvas and bottom toolbar
        container.append(&Separator::new(Orientation::Horizontal));

        // 2. Bottom Toolbar (Toolbar now placed cleanly at the bottom as requested)
        let toolbar = Box::new(Orientation::Horizontal, 6);
        toolbar.set_margin_start(8);
        toolbar.set_margin_end(8);
        toolbar.set_margin_top(4);
        toolbar.set_margin_bottom(4);

        // Page navigation buttons
        let btn_prev = Button::builder()
            .icon_name("go-previous-symbolic")
            .tooltip_text("Página Anterior")
            .css_classes(["flat"])
            .build();

        let btn_next = Button::builder()
            .icon_name("go-next-symbolic")
            .tooltip_text("Próxima Página")
            .css_classes(["flat"])
            .build();

        let page_label = Label::new(Some("Página: - / -"));
        page_label.set_margin_start(4);
        page_label.set_margin_end(4);

        toolbar.append(&btn_prev);
        toolbar.append(&btn_next);
        toolbar.append(&page_label);

        // Center Spacer
        let spacer = Box::new(Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        toolbar.append(&spacer);

        // View Mode Switcher: [Vertical Contínuo] / [Página Única]
        let mode_box = Box::new(Orientation::Horizontal, 0);
        mode_box.add_css_class("linked");

        let btn_mode_vertical = ToggleButton::builder()
            .icon_name("view-continuous-symbolic")
            .tooltip_text("Visualização Vertical Contínua (Rolagem contínua)")
            .active(true)
            .build();

        let btn_mode_single = ToggleButton::builder()
            .icon_name("view-paged-symbolic")
            .tooltip_text("Visualização de Página Única")
            .active(false)
            .build();

        mode_box.append(&btn_mode_vertical);
        mode_box.append(&btn_mode_single);
        toolbar.append(&mode_box);

        toolbar.append(&Separator::new(Orientation::Vertical));

        // Zoom Controls: [ - ] [ 100% ▾ ] [ + ]
        let zoom_box = Box::new(Orientation::Horizontal, 0);
        zoom_box.add_css_class("linked");

        let btn_zoom_out = Button::builder()
            .icon_name("zoom-out-symbolic")
            .tooltip_text("Diminuir Zoom (Ctrl -)")
            .build();

        let btn_zoom_menu = Button::builder()
            .label("100%")
            .tooltip_text("Opções Pré-definidas de Zoom")
            .build();

        let btn_zoom_in = Button::builder()
            .icon_name("zoom-in-symbolic")
            .tooltip_text("Aumentar Zoom (Ctrl +)")
            .build();

        zoom_box.append(&btn_zoom_out);
        zoom_box.append(&btn_zoom_menu);
        zoom_box.append(&btn_zoom_in);
        toolbar.append(&zoom_box);

        toolbar.append(&Separator::new(Orientation::Vertical));

        let btn_copy = Button::builder()
            .icon_name("edit-copy-symbolic")
            .tooltip_text("Copiar Seleção (Ctrl+C)")
            .css_classes(["flat", "circular"])
            .sensitive(false)
            .build();
        toolbar.append(&btn_copy);

        toolbar.append(&Separator::new(Orientation::Vertical));

        let btn_export_pdf = Button::builder()
            .icon_name("document-save-symbolic")
            .tooltip_text("Exportar / Salvar PDF...")
            .css_classes(["flat", "circular"])
            .build();
        toolbar.append(&btn_export_pdf);

        container.append(&toolbar);

        let on_toast: Rc<RefCell<Option<ToastCallback>>> = Rc::new(RefCell::new(None));
        let on_export_pdf: Rc<RefCell<Option<ExportCallback>>> = Rc::new(RefCell::new(None));

        {
            let on_exp_c = on_export_pdf.clone();
            btn_export_pdf.connect_clicked(move |_| {
                if let Some(ref cb) = *on_exp_c.borrow() {
                    cb();
                }
            });
        }

        let viewer = Self {
            container,
            drawing_area,
            scrolled,
            page_label,
            btn_zoom_menu,
            btn_mode_vertical: btn_mode_vertical.clone(),
            btn_mode_single: btn_mode_single.clone(),
            btn_copy: btn_copy.clone(),
            _btn_export_pdf: btn_export_pdf,
            state,
            on_toast,
            on_export_pdf,
        };

        viewer.setup_draw_and_actions(
            btn_prev,
            btn_next,
            btn_mode_vertical,
            btn_mode_single,
            btn_zoom_out,
            btn_zoom_in,
            btn_copy,
        );

        viewer
    }

    pub fn connect_toast<F: Fn(&str) + 'static>(&self, f: F) {
        *self.on_toast.borrow_mut() = Some(std::boxed::Box::new(f));
    }

    pub fn connect_export_pdf<F: Fn() + 'static>(&self, f: F) {
        *self.on_export_pdf.borrow_mut() = Some(std::boxed::Box::new(f));
    }

    pub fn widget(&self) -> &Box {
        &self.container
    }

    pub fn zoom(&self) -> f64 {
        self.state.borrow().zoom
    }

    pub fn set_zoom(&self, zoom: f64) {
        let zoom = zoom.clamp(0.25, 4.0);
        {
            let mut st = self.state.borrow_mut();
            st.zoom = zoom;
        }
        self.update_content_size();
        self.update_page_label();
        self.drawing_area.queue_draw();
    }

    pub fn view_mode_str(&self) -> String {
        match self.state.borrow().view_mode {
            PageViewMode::ContinuousVertical => "continuous".to_string(),
            PageViewMode::SinglePage => "single".to_string(),
        }
    }

    pub fn set_view_mode_str(&self, mode: &str) {
        if mode == "single" {
            self.btn_mode_single.set_active(true);
        } else {
            self.btn_mode_vertical.set_active(true);
        }
    }

    pub fn load_pdf(&self, path: &Path) -> bool {
        let uri = format!("file://{}", path.to_string_lossy());
        match Document::from_file(&uri, None) {
            Ok(doc) => {
                let n_pages = doc.n_pages() as usize;
                let mut links = Vec::new();
                for i in 0..n_pages {
                    if let Some(page) = doc.page(i as i32) {
                        let (_, ph) = page.size();
                        for mapping in page.link_mapping() {
                            if let Some(item) = extract_link_from_mapping(&doc, i, ph, &mapping) {
                                links.push(item);
                            }
                        }
                    }
                }
                let page_sizes: Vec<(f64, f64)> = (0..n_pages)
                    .map(|i| doc.page(i as i32).map(|p| p.size()).unwrap_or((595.0, 842.0)))
                    .collect();
                {
                    let mut st = self.state.borrow_mut();
                    st.document = Some(doc);
                    st.current_page = st.current_page.min(n_pages.saturating_sub(1));
                    st.total_pages = n_pages;
                    st.pdf_path = Some(path.to_path_buf());
                    st.links = links;
                    st.page_sizes = page_sizes;
                    st.page_cache.clear();
                    st.doc_generation = st.doc_generation.wrapping_add(1);
                    st.selection = None;
                }
                self.btn_copy.set_sensitive(false);
                self.update_content_size();
                self.update_page_label();
                self.drawing_area.queue_draw();
                Self::schedule_prewarm(&self.state, &self.drawing_area);
                true
            }
            Err(_) => false,
        }
    }

    /// Renders pages around the current page in idle time so that showing the
    /// viewer (e.g. switching from "Código" to "Ambos"/"Visual") is instant.
    fn schedule_prewarm(state: &Rc<RefCell<PdfState>>, drawing_area: &DrawingArea) {
        let (generation, order) = {
            let st = state.borrow();
            let total = st.total_pages;
            let cur = st.current_page;
            let mut order = Vec::with_capacity(PREWARM_PAGES);
            let mut d = 0usize;
            while order.len() < PREWARM_PAGES.min(total) {
                if cur + d < total {
                    order.push(cur + d);
                }
                if d > 0 && cur >= d && order.len() < PREWARM_PAGES.min(total) {
                    order.push(cur - d);
                }
                d += 1;
                if d > total {
                    break;
                }
            }
            (st.doc_generation, order)
        };

        let state_w = Rc::downgrade(state);
        let area_w = drawing_area.downgrade();
        let mut idx = 0usize;
        glib::idle_add_local_full(glib::Priority::LOW, move || {
            let (Some(state), Some(area)) = (state_w.upgrade(), area_w.upgrade()) else {
                return glib::ControlFlow::Break;
            };
            let Ok(mut st) = state.try_borrow_mut() else {
                return glib::ControlFlow::Continue;
            };
            if st.doc_generation != generation || idx >= order.len() {
                return glib::ControlFlow::Break;
            }
            let scale = area.scale_factor().max(1);
            let _ = st.page_surface(order[idx], scale);
            idx += 1;
            glib::ControlFlow::Continue
        });
    }

    fn update_content_size(&self) {
        Self::update_content_size_static(&self.state, &self.drawing_area);
    }

    fn update_content_size_static(state: &Rc<RefCell<PdfState>>, drawing_area: &DrawingArea) {
        let st = state.borrow();
        if let Some(ref doc) = st.document {
            let zoom = st.zoom;
            let spacing = 20.0;
            match st.view_mode {
                PageViewMode::ContinuousVertical => {
                    let mut total_h = spacing;
                    let mut max_w = 0.0f64;
                    for i in 0..st.total_pages {
                        if let Some(page) = doc.page(i as i32) {
                            let (pw, ph) = page.size();
                            let w = pw * zoom;
                            let h = ph * zoom;
                            if w > max_w {
                                max_w = w;
                            }
                            total_h += h + spacing;
                        }
                    }
                    drawing_area
                        .set_content_width((max_w + 40.0).max(200.0) as i32);
                    drawing_area
                        .set_content_height(total_h.max(200.0) as i32);
                }
                PageViewMode::SinglePage => {
                    if let Some(page) = doc.page(st.current_page as i32) {
                        let (pw, ph) = page.size();
                        drawing_area
                            .set_content_width((pw * zoom + 40.0).max(200.0) as i32);
                        drawing_area
                            .set_content_height((ph * zoom + 40.0).max(200.0) as i32);
                    }
                }
            }
        } else {
            drawing_area.set_content_width(200);
            drawing_area.set_content_height(200);
        }
    }

    fn update_page_label(&self) {
        Self::update_page_label_static(&self.state, &self.page_label, &self.btn_zoom_menu);
    }

    fn update_page_label_static(
        state: &Rc<RefCell<PdfState>>,
        page_label: &Label,
        btn_zoom_menu: &Button,
    ) {
        let st = state.borrow();
        if st.total_pages > 0 {
            page_label.set_text(&format!(
                "Página: {} / {}",
                st.current_page + 1,
                st.total_pages
            ));
            btn_zoom_menu.set_label(&format!("{:.0}%", st.zoom * 100.0));
        } else {
            page_label.set_text("Página: - / -");
            btn_zoom_menu.set_label("100%");
        }
    }

    fn setup_draw_and_actions(
        &self,
        btn_prev: Button,
        btn_next: Button,
        btn_mode_vertical: ToggleButton,
        btn_mode_single: ToggleButton,
        btn_zoom_out: Button,
        btn_zoom_in: Button,
        btn_copy: Button,
    ) {
        // --- 1. Cairo Draw Function ---
        let state = self.state.clone();
        let scrolled_draw = self.scrolled.clone();
        self.drawing_area
            .set_draw_func(move |area, cr, width, height| {
                // Never panic inside the draw callback if state is momentarily borrowed.
                let Ok(mut guard) = state.try_borrow_mut() else {
                    area.queue_draw();
                    return;
                };
                let st = &mut *guard;
                if st.document.is_some() {
                    let zoom = st.zoom;
                    let scale = area.scale_factor().max(1);
                    let inv = 1.0 / scale as f64;

                    // Fill canvas background
                    cr.set_source_rgb(0.92, 0.92, 0.94);
                    let _ = cr.paint();

                    // Paints one page (shadow + cached bitmap) at the given offset.
                    let paint_page = |st: &mut PdfState, idx: usize, ox: f64, oy: f64, tw: f64, th: f64| {
                        cr.save().ok();
                        cr.set_source_rgba(0.0, 0.0, 0.0, 0.18);
                        cr.rectangle(ox + 3.0, oy + 4.0, tw, th);
                        let _ = cr.fill();
                        cr.restore().ok();

                        if let Some(surf) = st.page_surface(idx, scale) {
                            cr.save().ok();
                            cr.translate(ox, oy);
                            cr.scale(inv, inv);
                            let _ = cr.set_source_surface(&surf, 0.0, 0.0);
                            let _ = cr.paint();
                            cr.restore().ok();
                        } else {
                            cr.set_source_rgb(1.0, 1.0, 1.0);
                            cr.rectangle(ox, oy, tw, th);
                            let _ = cr.fill();
                        }

                        // Paint text selection highlights
                        if let Some(ref sel) = st.selection {
                            for h in &sel.highlights {
                                if h.page_index == idx {
                                    cr.save().ok();
                                    cr.set_source_rgba(0.21, 0.52, 0.89, 0.35);
                                    for &(rx, ry, rw, rh) in &h.rects {
                                        cr.rectangle(ox + rx * zoom, oy + ry * zoom, rw * zoom, rh * zoom);
                                    }
                                    let _ = cr.fill();
                                    cr.restore().ok();
                                }
                            }
                        }
                    };

                    match st.view_mode {
                        PageViewMode::SinglePage => {
                            let idx = st.current_page;
                            if let Some(&(pw, ph)) = st.page_sizes.get(idx) {
                                let tw = pw * zoom;
                                let th = ph * zoom;
                                let ox = (width as f64 - tw).max(0.0) / 2.0;
                                let oy = (height as f64 - th).max(0.0) / 2.0;
                                paint_page(st, idx, ox, oy, tw, th);
                                return;
                            }
                        }
                        PageViewMode::ContinuousVertical => {
                            // GTK4 hands us a clip covering the whole (huge) canvas, so cull
                            // against the actual scrolled viewport instead.
                            let vadj = scrolled_draw.vadjustment();
                            let (view_top, view_bot) = if vadj.page_size() > 0.0 {
                                (vadj.value() - 100.0, vadj.value() + vadj.page_size() + 100.0)
                            } else {
                                (0.0, height as f64)
                            };
                            let spacing = 20.0;
                            let mut cur_y = spacing;
                            let sizes = st.page_sizes.clone();
                            for (i, &(pw, ph)) in sizes.iter().enumerate() {
                                let tw = pw * zoom;
                                let th = ph * zoom;
                                if cur_y > view_bot {
                                    break;
                                }
                                if cur_y + th >= view_top {
                                    let ox = (width as f64 - tw).max(0.0) / 2.0;
                                    paint_page(st, i, ox, cur_y, tw, th);
                                }
                                cur_y += th + spacing;
                            }
                            return;
                        }
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

        // --- 2. Dynamic Scroll Position Tracking (Updates current page as user scrolls) ---
        let vadj = self.scrolled.vadjustment();
        {
            let area_ps = self.drawing_area.clone();
            vadj.connect_page_size_notify(move |_| area_ps.queue_draw());
        }
        let state_scroll = self.state.clone();
        let label_scroll = self.page_label.clone();
        let area_scroll = self.drawing_area.clone();
        vadj.connect_value_changed(move |adj| {
            area_scroll.queue_draw();
            // `set_value()` is called from handlers that already hold a mutable
            // borrow (mode toggle, prev/next); those update current_page themselves.
            let Ok(mut st) = state_scroll.try_borrow_mut() else {
                return;
            };
            if st.view_mode == PageViewMode::ContinuousVertical && st.total_pages > 0 {
                if let Some(ref doc) = st.document {
                    let scroll_y = adj.value();
                    let page_size = adj.page_size();
                    let mid_y = scroll_y + (page_size / 2.0).min(350.0);
                    let spacing = 20.0;
                    let mut cur_y = spacing;
                    let zoom = st.zoom;

                    for i in 0..st.total_pages {
                        if let Some(page) = doc.page(i as i32) {
                            let (_, ph) = page.size();
                            let th = ph * zoom;
                            if mid_y >= cur_y && mid_y <= cur_y + th + spacing {
                                if st.current_page != i {
                                    st.current_page = i;
                                    label_scroll.set_text(&format!(
                                        "Página: {} / {}",
                                        i + 1,
                                        st.total_pages
                                    ));
                                }
                                break;
                            }
                            cur_y += th + spacing;
                        }
                    }
                }
            }
        });

        // --- 3. Page Navigation (Prev / Next) ---
        let state_prev = self.state.clone();
        let draw_prev = self.drawing_area.clone();
        let label_prev = self.page_label.clone();
        let scrolled_prev = self.scrolled.clone();
        btn_prev.connect_clicked(move |_| {
            let mut st = state_prev.borrow_mut();
            if st.current_page > 0 {
                st.current_page -= 1;
                let cur = st.current_page;
                let tot = st.total_pages;
                let mode = st.view_mode;
                let zoom = st.zoom;
                label_prev.set_text(&format!("Página: {} / {}", cur + 1, tot));

                if mode == PageViewMode::ContinuousVertical {
                    if let Some(ref doc) = st.document {
                        let spacing = 20.0;
                        let mut target_y = spacing;
                        for i in 0..cur {
                            if let Some(p) = doc.page(i as i32) {
                                let (_, ph) = p.size();
                                target_y += ph * zoom + spacing;
                            }
                        }
                        scrolled_prev.vadjustment().set_value(target_y);
                    }
                } else {
                    if let Some(ref doc) = st.document {
                        if let Some(page) = doc.page(cur as i32) {
                            let (pw, ph) = page.size();
                            draw_prev.set_content_width((pw * zoom + 40.0) as i32);
                            draw_prev.set_content_height((ph * zoom + 40.0) as i32);
                        }
                    }
                    draw_prev.queue_draw();
                }
            }
        });

        let state_next = self.state.clone();
        let draw_next = self.drawing_area.clone();
        let label_next = self.page_label.clone();
        let scrolled_next = self.scrolled.clone();
        btn_next.connect_clicked(move |_| {
            let mut st = state_next.borrow_mut();
            if st.total_pages > 0 && st.current_page + 1 < st.total_pages {
                st.current_page += 1;
                let cur = st.current_page;
                let tot = st.total_pages;
                let mode = st.view_mode;
                let zoom = st.zoom;
                label_next.set_text(&format!("Página: {} / {}", cur + 1, tot));

                if mode == PageViewMode::ContinuousVertical {
                    if let Some(ref doc) = st.document {
                        let spacing = 20.0;
                        let mut target_y = spacing;
                        for i in 0..cur {
                            if let Some(p) = doc.page(i as i32) {
                                let (_, ph) = p.size();
                                target_y += ph * zoom + spacing;
                            }
                        }
                        scrolled_next.vadjustment().set_value(target_y);
                    }
                } else {
                    if let Some(ref doc) = st.document {
                        if let Some(page) = doc.page(cur as i32) {
                            let (pw, ph) = page.size();
                            draw_next.set_content_width((pw * zoom + 40.0) as i32);
                            draw_next.set_content_height((ph * zoom + 40.0) as i32);
                        }
                    }
                    draw_next.queue_draw();
                }
            }
        });

        // --- 4. View Mode Switching (Continuous Vertical vs Single Page) ---
        let state_mode_v = self.state.clone();
        let draw_mode_v = self.drawing_area.clone();
        let btn_single_ref = btn_mode_single.clone();
        let scrolled_mode_v = self.scrolled.clone();
        btn_mode_vertical.connect_toggled(move |btn| {
            if btn.is_active() {
                btn_single_ref.set_active(false);
                let mut st = state_mode_v.borrow_mut();
                st.view_mode = PageViewMode::ContinuousVertical;

                if let Some(ref doc) = st.document {
                    let zoom = st.zoom;
                    let spacing = 20.0;
                    let mut total_h = spacing;
                    let mut max_w = 0.0f64;
                    for i in 0..st.total_pages {
                        if let Some(page) = doc.page(i as i32) {
                            let (pw, ph) = page.size();
                            let w = pw * zoom;
                            let h = ph * zoom;
                            if w > max_w {
                                max_w = w;
                            }
                            total_h += h + spacing;
                        }
                    }
                    draw_mode_v.set_content_width((max_w + 40.0).max(200.0) as i32);
                    draw_mode_v.set_content_height(total_h.max(200.0) as i32);

                    let cur = st.current_page;
                    let mut target_y = spacing;
                    for i in 0..cur {
                        if let Some(p) = doc.page(i as i32) {
                            let (_, ph) = p.size();
                            target_y += ph * zoom + spacing;
                        }
                    }
                    scrolled_mode_v.vadjustment().set_value(target_y);
                }
                draw_mode_v.queue_draw();
            }
        });

        let state_mode_s = self.state.clone();
        let draw_mode_s = self.drawing_area.clone();
        let btn_vertical_ref = btn_mode_vertical.clone();
        btn_mode_single.connect_toggled(move |btn| {
            if btn.is_active() {
                btn_vertical_ref.set_active(false);
                let mut st = state_mode_s.borrow_mut();
                st.view_mode = PageViewMode::SinglePage;

                if let Some(ref doc) = st.document {
                    let zoom = st.zoom;
                    if let Some(page) = doc.page(st.current_page as i32) {
                        let (pw, ph) = page.size();
                        draw_mode_s.set_content_width((pw * zoom + 40.0).max(200.0) as i32);
                        draw_mode_s.set_content_height((ph * zoom + 40.0).max(200.0) as i32);
                    }
                }
                draw_mode_s.queue_draw();
            }
        });

        // --- 5. Predefined Zoom Popover Options ---
        let pop_zoom = Popover::new();
        pop_zoom.set_parent(&self.btn_zoom_menu);

        let pop_box = Box::new(Orientation::Vertical, 2);
        pop_box.set_margin_start(6);
        pop_box.set_margin_end(6);
        pop_box.set_margin_top(6);
        pop_box.set_margin_bottom(6);

        // Preset items list: (Label, ZoomMultiplier or Special)
        let presets = [
            ("Ajustar à Largura", 0.0), // 0.0 indicates fit width
            ("Ajustar à Página", -1.0), // -1.0 indicates fit page
            ("50%", 0.50),
            ("75%", 0.75),
            ("100% (Real)", 1.00),
            ("125%", 1.25),
            ("150%", 1.50),
            ("200%", 2.00),
            ("300%", 3.00),
        ];

        for (label, val) in presets {
            if val == 0.50 {
                pop_box.append(&Separator::new(Orientation::Horizontal));
            }

            let btn_preset = Button::builder()
                .label(label)
                .css_classes(["flat"])
                .build();

            let pop_c = pop_zoom.clone();
            let state_p = self.state.clone();
            let draw_p = self.drawing_area.clone();
            let btn_zoom_p = self.btn_zoom_menu.clone();
            let scrolled_p = self.scrolled.clone();

            btn_preset.connect_clicked(move |_| {
                let mut st = state_p.borrow_mut();
                if val == 0.0 {
                    // Fit Width
                    if let Some(ref doc) = st.document {
                        if let Some(page) = doc.page(st.current_page as i32) {
                            let (pw, _) = page.size();
                            let avail_w = (scrolled_p.width() - 48).max(200) as f64;
                            st.zoom = (avail_w / pw).clamp(0.2, 5.0);
                        }
                    }
                } else if val == -1.0 {
                    // Fit Page
                    if let Some(ref doc) = st.document {
                        if let Some(page) = doc.page(st.current_page as i32) {
                            let (pw, ph) = page.size();
                            let avail_w = (scrolled_p.width() - 48).max(200) as f64;
                            let avail_h = (scrolled_p.height() - 48).max(200) as f64;
                            let zw = avail_w / pw;
                            let zh = avail_h / ph;
                            st.zoom = zw.min(zh).clamp(0.2, 5.0);
                        }
                    }
                } else {
                    st.zoom = val;
                }

                btn_zoom_p.set_label(&format!("{:.0}%", st.zoom * 100.0));

                // Recalculate content size
                if let Some(ref doc) = st.document {
                    let zoom = st.zoom;
                    let spacing = 20.0;
                    match st.view_mode {
                        PageViewMode::ContinuousVertical => {
                            let mut total_h = spacing;
                            let mut max_w = 0.0f64;
                            for i in 0..st.total_pages {
                                if let Some(page) = doc.page(i as i32) {
                                    let (pw, ph) = page.size();
                                    let w = pw * zoom;
                                    let h = ph * zoom;
                                    if w > max_w {
                                        max_w = w;
                                    }
                                    total_h += h + spacing;
                                }
                            }
                            draw_p.set_content_width((max_w + 40.0).max(200.0) as i32);
                            draw_p.set_content_height(total_h.max(200.0) as i32);
                        }
                        PageViewMode::SinglePage => {
                            if let Some(page) = doc.page(st.current_page as i32) {
                                let (pw, ph) = page.size();
                                draw_p.set_content_width((pw * zoom + 40.0).max(200.0) as i32);
                                draw_p.set_content_height((ph * zoom + 40.0).max(200.0) as i32);
                            }
                        }
                    }
                }

                draw_p.queue_draw();
                pop_c.popdown();
            });

            pop_box.append(&btn_preset);
        }

        pop_zoom.set_child(Some(&pop_box));

        {
            let pop_open = pop_zoom.clone();
            self.btn_zoom_menu.connect_clicked(move |_| {
                pop_open.popup();
            });
        }

        // --- 6. Step Zoom In / Zoom Out ---
        let state_in = self.state.clone();
        let draw_in = self.drawing_area.clone();
        let btn_zoom_in_lbl = self.btn_zoom_menu.clone();
        btn_zoom_in.connect_clicked(move |_| {
            let mut st = state_in.borrow_mut();
            st.zoom = (st.zoom * 1.2).min(5.0);
            let zoom = st.zoom;
            btn_zoom_in_lbl.set_label(&format!("{:.0}%", zoom * 100.0));

            if let Some(ref doc) = st.document {
                let spacing = 20.0;
                match st.view_mode {
                    PageViewMode::ContinuousVertical => {
                        let mut total_h = spacing;
                        let mut max_w = 0.0f64;
                        for i in 0..st.total_pages {
                            if let Some(page) = doc.page(i as i32) {
                                let (pw, ph) = page.size();
                                let w = pw * zoom;
                                let h = ph * zoom;
                                if w > max_w {
                                    max_w = w;
                                }
                                total_h += h + spacing;
                            }
                        }
                        draw_in.set_content_width((max_w + 40.0).max(200.0) as i32);
                        draw_in.set_content_height(total_h.max(200.0) as i32);
                    }
                    PageViewMode::SinglePage => {
                        if let Some(page) = doc.page(st.current_page as i32) {
                            let (pw, ph) = page.size();
                            draw_in.set_content_width((pw * zoom + 40.0).max(200.0) as i32);
                            draw_in.set_content_height((ph * zoom + 40.0).max(200.0) as i32);
                        }
                    }
                }
            }
            draw_in.queue_draw();
        });

        let state_out = self.state.clone();
        let draw_out = self.drawing_area.clone();
        let btn_zoom_out_lbl = self.btn_zoom_menu.clone();
        btn_zoom_out.connect_clicked(move |_| {
            let mut st = state_out.borrow_mut();
            st.zoom = (st.zoom / 1.2).max(0.2);
            let zoom = st.zoom;
            btn_zoom_out_lbl.set_label(&format!("{:.0}%", zoom * 100.0));

            if let Some(ref doc) = st.document {
                let spacing = 20.0;
                match st.view_mode {
                    PageViewMode::ContinuousVertical => {
                        let mut total_h = spacing;
                        let mut max_w = 0.0f64;
                        for i in 0..st.total_pages {
                            if let Some(page) = doc.page(i as i32) {
                                let (pw, ph) = page.size();
                                let w = pw * zoom;
                                let h = ph * zoom;
                                if w > max_w {
                                    max_w = w;
                                }
                                total_h += h + spacing;
                            }
                        }
                        draw_out.set_content_width((max_w + 40.0).max(200.0) as i32);
                        draw_out.set_content_height(total_h.max(200.0) as i32);
                    }
                    PageViewMode::SinglePage => {
                        if let Some(page) = doc.page(st.current_page as i32) {
                            let (pw, ph) = page.size();
                            draw_out.set_content_width((pw * zoom + 40.0).max(200.0) as i32);
                            draw_out.set_content_height((ph * zoom + 40.0).max(200.0) as i32);
                        }
                    }
                }
            }
            draw_out.queue_draw();
        });

        // --- 6. Link Hover Motion Controller ---
        let motion = EventControllerMotion::new();
        let state_motion = self.state.clone();
        motion.connect_motion(move |ctrl, x, y| {
            if let Some(widget) = ctrl.widget() {
                let w = widget.width() as f64;
                let h = widget.height() as f64;
                if let Some(link) = Self::find_link_with_state(&state_motion, x, y, w, h) {
                    widget.set_cursor_from_name(Some("pointer"));
                    widget.set_tooltip_text(Some(&link.tooltip));
                } else {
                    let st = state_motion.borrow();
                    if is_point_over_page(&st, x, y, w, h) {
                        widget.set_cursor_from_name(Some("text"));
                    } else {
                        widget.set_cursor_from_name(None);
                    }
                    widget.set_has_tooltip(false);
                }
            }
        });

        motion.connect_leave(move |ctrl| {
            if let Some(widget) = ctrl.widget() {
                widget.set_cursor_from_name(None);
                widget.set_has_tooltip(false);
            }
        });
        self.drawing_area.add_controller(motion);

        // --- 7. Mouse Drag Selection Controller ---
        let drag = GestureDrag::new();
        drag.set_button(1);
        let drag_start_coords: Rc<RefCell<Option<(f64, f64)>>> = Rc::new(RefCell::new(None));
        let drag_has_moved: Rc<RefCell<bool>> = Rc::new(RefCell::new(false));

        let ds_b = drag_start_coords.clone();
        let dm_b = drag_has_moved.clone();
        let draw_focus = self.drawing_area.clone();
        drag.connect_drag_begin(move |_gesture, x, y| {
            draw_focus.grab_focus();
            *ds_b.borrow_mut() = Some((x, y));
            *dm_b.borrow_mut() = false;
        });

        let ds_u = drag_start_coords.clone();
        let dm_u = drag_has_moved.clone();
        let state_u = self.state.clone();
        let draw_u = self.drawing_area.clone();
        let btn_copy_u = btn_copy.clone();
        drag.connect_drag_update(move |_gesture, offset_x, offset_y| {
            if offset_x.hypot(offset_y) < 3.0 {
                return;
            }
            *dm_u.borrow_mut() = true;
            let Some((start_x, start_y)) = *ds_u.borrow() else { return };
            let cur_x = start_x + offset_x;
            let cur_y = start_y + offset_y;
            let w = draw_u.width() as f64;
            let h = draw_u.height() as f64;

            let mut st = state_u.borrow_mut();
            if let Some(ref doc) = st.document {
                let start_info = point_to_page_coords(&st, start_x, start_y, w, h);
                let cur_info = point_to_page_coords(&st, cur_x, cur_y, w, h);
                if let (Some((sp, sx, sy)), Some((cp, cx, cy))) = (start_info, cur_info) {
                    let page_sizes = st.page_sizes.clone();
                    let sel = compute_selection_for_range(
                        doc,
                        &page_sizes,
                        sp,
                        (sx, sy),
                        cp,
                        (cx, cy),
                        SelectionStyle::Glyph,
                    );
                    let has_text = sel.as_ref().map_or(false, |s| !s.text.trim().is_empty());
                    btn_copy_u.set_sensitive(has_text);
                    st.selection = sel;
                    draw_u.queue_draw();
                }
            }
        });

        let dm_e = drag_has_moved.clone();
        let state_e = self.state.clone();
        let draw_e = self.drawing_area.clone();
        let btn_copy_e = btn_copy.clone();
        drag.connect_drag_end(move |_gesture, offset_x, offset_y| {
            if offset_x.hypot(offset_y) < 3.0 && !*dm_e.borrow() {
                let mut st = state_e.borrow_mut();
                if st.selection.is_some() {
                    st.selection = None;
                    btn_copy_e.set_sensitive(false);
                    draw_e.queue_draw();
                }
            }
        });
        self.drawing_area.add_controller(drag);

        // --- 8. Link Click & Word/Line Selection Controller ---
        let click = GestureClick::new();
        click.set_button(1);
        let state_click = self.state.clone();
        let scrolled_click = self.scrolled.clone();
        let draw_click = self.drawing_area.clone();
        let page_label_click = self.page_label.clone();
        let zoom_menu_click = self.btn_zoom_menu.clone();
        let toast_click = self.on_toast.clone();
        let btn_copy_click = btn_copy.clone();

        click.connect_pressed(move |gesture, n_press, x, y| {
            if let Some(widget) = gesture.widget() {
                let w = widget.width() as f64;
                let h = widget.height() as f64;
                if n_press == 1 {
                    if let Some(link) = Self::find_link_with_state(&state_click, x, y, w, h) {
                        Self::handle_link_click(
                            &state_click,
                            &scrolled_click,
                            &draw_click,
                            &page_label_click,
                            &zoom_menu_click,
                            &toast_click,
                            &link,
                        );
                    }
                } else if n_press == 2 {
                    // Double click: Select Word
                    let mut st = state_click.borrow_mut();
                    if let Some(ref doc) = st.document {
                        if let Some((p, px, py)) = point_to_page_coords(&st, x, y, w, h) {
                            let page_sizes = st.page_sizes.clone();
                            let sel = compute_selection_for_range(
                                doc,
                                &page_sizes,
                                p,
                                (px - 2.0, py - 2.0),
                                p,
                                (px + 2.0, py + 2.0),
                                SelectionStyle::Word,
                            );
                            let has_text = sel.as_ref().map_or(false, |s| !s.text.trim().is_empty());
                            btn_copy_click.set_sensitive(has_text);
                            st.selection = sel;
                            draw_click.queue_draw();
                        }
                    }
                } else if n_press == 3 {
                    // Triple click: Select Line
                    let mut st = state_click.borrow_mut();
                    if let Some(ref doc) = st.document {
                        if let Some((p, px, py)) = point_to_page_coords(&st, x, y, w, h) {
                            let page_sizes = st.page_sizes.clone();
                            let sel = compute_selection_for_range(
                                doc,
                                &page_sizes,
                                p,
                                (px - 2.0, py - 2.0),
                                p,
                                (px + 2.0, py + 2.0),
                                SelectionStyle::Line,
                            );
                            let has_text = sel.as_ref().map_or(false, |s| !s.text.trim().is_empty());
                            btn_copy_click.set_sensitive(has_text);
                            st.selection = sel;
                            draw_click.queue_draw();
                        }
                    }
                }
            }
        });
        self.drawing_area.add_controller(click);

        // --- 9. Right-click Context Menu Popover ---
        let right_click = GestureClick::new();
        right_click.set_button(3);
        let pop_context = Popover::new();
        pop_context.set_parent(&self.drawing_area);
        pop_context.set_has_arrow(true);

        let state_rc = self.state.clone();
        let draw_rc = self.drawing_area.clone();
        let on_toast_rc = self.on_toast.clone();
        let btn_copy_rc = btn_copy.clone();
        let pop_c = pop_context.clone();

        right_click.connect_pressed(move |gesture, n_press, x, y| {
            if n_press != 1 {
                return;
            }
            gesture.set_state(gtk4::EventSequenceState::Claimed);

            let has_sel = {
                let st = state_rc.borrow();
                st.selection.as_ref().map_or(false, |s| !s.text.trim().is_empty())
            };

            let menu_box = Box::new(Orientation::Vertical, 2);
            menu_box.set_margin_start(4);
            menu_box.set_margin_end(4);
            menu_box.set_margin_top(4);
            menu_box.set_margin_bottom(4);

            let btn_copy_item = Button::builder()
                .label("Copiar Seleção (Ctrl+C)")
                .icon_name("edit-copy-symbolic")
                .css_classes(["flat"])
                .sensitive(has_sel)
                .build();

            let pop_cp = pop_c.clone();
            let st_cp = state_rc.clone();
            let toast_cp = on_toast_rc.clone();
            btn_copy_item.connect_clicked(move |_| {
                pop_cp.popdown();
                Self::copy_selection_to_clipboard_static(&st_cp, &toast_cp);
            });
            menu_box.append(&btn_copy_item);

            let btn_sel_all = Button::builder()
                .label("Selecionar Tudo (Ctrl+A)")
                .icon_name("edit-select-all-symbolic")
                .css_classes(["flat"])
                .build();

            let pop_sa = pop_c.clone();
            let st_sa = state_rc.clone();
            let draw_sa = draw_rc.clone();
            let copy_sa = btn_copy_rc.clone();
            btn_sel_all.connect_clicked(move |_| {
                pop_sa.popdown();
                Self::select_all_static(&st_sa, &draw_sa, &copy_sa);
            });
            menu_box.append(&btn_sel_all);

            if has_sel {
                let btn_clear = Button::builder()
                    .label("Limpar Seleção (Esc)")
                    .icon_name("edit-clear-symbolic")
                    .css_classes(["flat"])
                    .build();

                let pop_cl = pop_c.clone();
                let st_cl = state_rc.clone();
                let draw_cl = draw_rc.clone();
                let copy_cl = btn_copy_rc.clone();
                btn_clear.connect_clicked(move |_| {
                    pop_cl.popdown();
                    Self::clear_selection_static(&st_cl, &draw_cl, &copy_cl);
                });
                menu_box.append(&btn_clear);
            }

            pop_c.set_child(Some(&menu_box));
            let target_rect = gdk4::Rectangle::new(x as i32, y as i32, 1, 1);
            pop_c.set_pointing_to(Some(&target_rect));
            pop_c.popup();
        });
        self.drawing_area.add_controller(right_click);

        // --- 10. Keyboard Shortcut Controller (Ctrl+C, Ctrl+A, Esc) ---
        let key_ctrl = EventControllerKey::new();
        let state_key = self.state.clone();
        let draw_key = self.drawing_area.clone();
        let on_toast_key = self.on_toast.clone();
        let btn_copy_key = btn_copy.clone();
        key_ctrl.connect_key_pressed(move |_ctrl, keyval, _keycode, modifier| {
            let is_ctrl = modifier.contains(gdk4::ModifierType::CONTROL_MASK);
            if is_ctrl {
                if keyval == gdk4::Key::c || keyval == gdk4::Key::C || keyval == gdk4::Key::Insert {
                    if Self::copy_selection_to_clipboard_static(&state_key, &on_toast_key) {
                        return glib::Propagation::Stop;
                    }
                } else if keyval == gdk4::Key::a || keyval == gdk4::Key::A {
                    Self::select_all_static(&state_key, &draw_key, &btn_copy_key);
                    return glib::Propagation::Stop;
                }
            } else if keyval == gdk4::Key::Escape {
                Self::clear_selection_static(&state_key, &draw_key, &btn_copy_key);
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        self.drawing_area.add_controller(key_ctrl);

        // --- 11. Toolbar Copy Button Action ---
        let state_btn_copy = self.state.clone();
        let on_toast_btn_copy = self.on_toast.clone();
        btn_copy.connect_clicked(move |_| {
            Self::copy_selection_to_clipboard_static(&state_btn_copy, &on_toast_btn_copy);
        });
    }

    fn find_link_with_state(
        state: &Rc<RefCell<PdfState>>,
        mouse_x: f64,
        mouse_y: f64,
        widget_width: f64,
        widget_height: f64,
    ) -> Option<PdfLinkItem> {
        let st = state.borrow();
        let doc = st.document.as_ref()?;
        let zoom = st.zoom;

        match st.view_mode {
            PageViewMode::SinglePage => {
                let page = doc.page(st.current_page as i32)?;
                let (pw, ph) = page.size();
                let target_w = pw * zoom;
                let target_h = ph * zoom;
                let offset_x = (widget_width - target_w).max(0.0) / 2.0;
                let offset_y = (widget_height - target_h).max(0.0) / 2.0;

                let page_x = (mouse_x - offset_x) / zoom;
                let page_y = (mouse_y - offset_y) / zoom;

                if page_x >= 0.0 && page_x <= pw && page_y >= 0.0 && page_y <= ph {
                    for link in &st.links {
                        if link.page_index == st.current_page {
                            let (x1, y1, x2, y2) = link.rect;
                            if page_x >= x1 && page_x <= x2 && page_y >= y1 && page_y <= y2 {
                                return Some(link.clone());
                            }
                        }
                    }
                }
            }
            PageViewMode::ContinuousVertical => {
                let spacing = 20.0;
                let mut cur_y = spacing;
                for i in 0..st.total_pages {
                    if let Some(page) = doc.page(i as i32) {
                        let (pw, ph) = page.size();
                        let target_w = pw * zoom;
                        let target_h = ph * zoom;
                        let offset_x = (widget_width - target_w).max(0.0) / 2.0;
                        let offset_y = cur_y;

                        if mouse_y >= offset_y && mouse_y <= offset_y + target_h {
                            let page_x = (mouse_x - offset_x) / zoom;
                            let page_y = (mouse_y - offset_y) / zoom;

                            if page_x >= 0.0 && page_x <= pw {
                                for link in &st.links {
                                    if link.page_index == i {
                                        let (x1, y1, x2, y2) = link.rect;
                                        if page_x >= x1 && page_x <= x2 && page_y >= y1 && page_y <= y2 {
                                            return Some(link.clone());
                                        }
                                    }
                                }
                            }
                            return None;
                        }
                        cur_y += target_h + spacing;
                    }
                }
            }
        }
        None
    }

    fn handle_link_click(
        state: &Rc<RefCell<PdfState>>,
        scrolled: &ScrolledWindow,
        drawing_area: &DrawingArea,
        page_label: &Label,
        btn_zoom_menu: &Button,
        on_toast: &Rc<RefCell<Option<ToastCallback>>>,
        link: &PdfLinkItem,
    ) {
        match &link.action {
            PdfLinkAction::Goto {
                page_index,
                top_offset,
            } => {
                let target_page = *page_index;
                let top_off = *top_offset;
                let (view_mode, total_pages, zoom, doc_opt) = {
                    let st = state.borrow();
                    (st.view_mode, st.total_pages, st.zoom, st.document.clone())
                };

                if target_page >= total_pages {
                    return;
                }

                match view_mode {
                    PageViewMode::ContinuousVertical => {
                        if let Some(ref doc) = doc_opt {
                            let spacing = 20.0;
                            let mut target_y = spacing;
                            for i in 0..target_page {
                                if let Some(p) = doc.page(i as i32) {
                                    let (_, ph) = p.size();
                                    target_y += ph * zoom + spacing;
                                }
                            }
                            target_y += top_off * zoom;
                            let scroll_val = (target_y - 30.0).max(0.0);
                            scrolled.vadjustment().set_value(scroll_val);
                        }
                        {
                            let mut st = state.borrow_mut();
                            st.current_page = target_page;
                        }
                        Self::update_page_label_static(state, page_label, btn_zoom_menu);
                    }
                    PageViewMode::SinglePage => {
                        {
                            let mut st = state.borrow_mut();
                            st.current_page = target_page;
                        }
                        Self::update_content_size_static(state, drawing_area);
                        Self::update_page_label_static(state, page_label, btn_zoom_menu);
                        drawing_area.queue_draw();
                    }
                }
            }
            PdfLinkAction::Uri(uri_str) => {
                if let Some(ref cb) = *on_toast.borrow() {
                    if uri_str.starts_with("mailto:") {
                        cb(&format!(
                            "Abrindo cliente de e-mail para {}",
                            uri_str.trim_start_matches("mailto:")
                        ));
                    } else {
                        cb(&format!("Abrindo link: {}", uri_str));
                    }
                }

                if uri_str.starts_with("mailto:") {
                    let _ = std::process::Command::new("xdg-open").arg(uri_str).spawn();
                } else if let Err(_) = gtk4::gio::AppInfo::launch_default_for_uri(
                    uri_str,
                    None::<&gtk4::gio::AppLaunchContext>,
                ) {
                    let _ = std::process::Command::new("xdg-open").arg(uri_str).spawn();
                }
            }
        }
    }

    pub fn copy_selection_to_clipboard(&self) -> bool {
        Self::copy_selection_to_clipboard_static(&self.state, &self.on_toast)
    }

    pub fn copy_selection_to_clipboard_static(
        state: &Rc<RefCell<PdfState>>,
        on_toast: &Rc<RefCell<Option<ToastCallback>>>,
    ) -> bool {
        let text = {
            let st = state.borrow();
            st.selection.as_ref().map(|s| s.text.clone())
        };
        if let Some(text) = text {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                if let Some(display) = gdk4::Display::default() {
                    display.clipboard().set_text(trimmed);
                    if let Some(ref cb) = *on_toast.borrow() {
                        cb("Texto copiado para a área de transferência");
                    }
                    return true;
                }
            }
        }
        false
    }

    pub fn clear_selection(&self) {
        Self::clear_selection_static(&self.state, &self.drawing_area, &self.btn_copy);
    }

    pub fn clear_selection_static(
        state: &Rc<RefCell<PdfState>>,
        drawing_area: &DrawingArea,
        btn_copy: &Button,
    ) {
        let mut st = state.borrow_mut();
        st.selection = None;
        btn_copy.set_sensitive(false);
        drawing_area.queue_draw();
    }

    pub fn select_all(&self) {
        Self::select_all_static(&self.state, &self.drawing_area, &self.btn_copy);
    }

    pub fn select_all_static(
        state: &Rc<RefCell<PdfState>>,
        drawing_area: &DrawingArea,
        btn_copy: &Button,
    ) {
        let mut st = state.borrow_mut();
        if let Some(ref doc) = st.document {
            let total = st.total_pages;
            if total == 0 {
                return;
            }
            let page_sizes = st.page_sizes.clone();
            let (start_page, end_page, end_pt) = match st.view_mode {
                PageViewMode::SinglePage => {
                    let cur = st.current_page;
                    let (pw, ph) = page_sizes.get(cur).copied().unwrap_or((595.0, 842.0));
                    (cur, cur, (pw, ph))
                }
                PageViewMode::ContinuousVertical => {
                    let last = total - 1;
                    let (pw, ph) = page_sizes.get(last).copied().unwrap_or((595.0, 842.0));
                    (0, last, (pw, ph))
                }
            };
            let sel = compute_selection_for_range(
                doc,
                &page_sizes,
                start_page,
                (0.0, 0.0),
                end_page,
                end_pt,
                SelectionStyle::Glyph,
            );
            let has_text = sel.as_ref().map_or(false, |s| !s.text.trim().is_empty());
            btn_copy.set_sensitive(has_text);
            st.selection = sel;
            drawing_area.queue_draw();
        }
    }

    pub fn has_selection(&self) -> bool {
        let st = self.state.borrow();
        st.selection.as_ref().map_or(false, |s| !s.text.trim().is_empty())
    }

    pub fn selected_text(&self) -> Option<String> {
        let st = self.state.borrow();
        st.selection.as_ref().map(|s| s.text.clone())
    }
}

fn is_point_over_page(
    st: &PdfState,
    mouse_x: f64,
    mouse_y: f64,
    widget_width: f64,
    widget_height: f64,
) -> bool {
    if st.total_pages == 0 {
        return false;
    }
    let zoom = st.zoom;
    match st.view_mode {
        PageViewMode::SinglePage => {
            let idx = st.current_page;
            if let Some(&(pw, ph)) = st.page_sizes.get(idx) {
                let target_w = pw * zoom;
                let target_h = ph * zoom;
                let offset_x = (widget_width - target_w).max(0.0) / 2.0;
                let offset_y = (widget_height - target_h).max(0.0) / 2.0;
                mouse_x >= offset_x
                    && mouse_x <= offset_x + target_w
                    && mouse_y >= offset_y
                    && mouse_y <= offset_y + target_h
            } else {
                false
            }
        }
        PageViewMode::ContinuousVertical => {
            let spacing = 20.0;
            let mut cur_y = spacing;
            for i in 0..st.total_pages {
                if let Some(&(pw, ph)) = st.page_sizes.get(i) {
                    let target_w = pw * zoom;
                    let target_h = ph * zoom;
                    let offset_x = (widget_width - target_w).max(0.0) / 2.0;
                    if mouse_y >= cur_y
                        && mouse_y <= cur_y + target_h
                        && mouse_x >= offset_x
                        && mouse_x <= offset_x + target_w
                    {
                        return true;
                    }
                    cur_y += target_h + spacing;
                }
            }
            false
        }
    }
}

fn point_to_page_coords(
    st: &PdfState,
    mouse_x: f64,
    mouse_y: f64,
    widget_width: f64,
    widget_height: f64,
) -> Option<(usize, f64, f64)> {
    if st.total_pages == 0 {
        return None;
    }
    let zoom = st.zoom;
    match st.view_mode {
        PageViewMode::SinglePage => {
            let idx = st.current_page;
            let &(pw, ph) = st.page_sizes.get(idx)?;
            let target_w = pw * zoom;
            let target_h = ph * zoom;
            let offset_x = (widget_width - target_w).max(0.0) / 2.0;
            let offset_y = (widget_height - target_h).max(0.0) / 2.0;

            let page_x = ((mouse_x - offset_x) / zoom).clamp(0.0, pw);
            let page_y = ((mouse_y - offset_y) / zoom).clamp(0.0, ph);
            Some((idx, page_x, page_y))
        }
        PageViewMode::ContinuousVertical => {
            let spacing = 20.0;
            let mut cur_y = spacing;
            let total = st.total_pages;

            if mouse_y < spacing {
                let &(pw, _) = st.page_sizes.first()?;
                let target_w = pw * zoom;
                let offset_x = (widget_width - target_w).max(0.0) / 2.0;
                let page_x = ((mouse_x - offset_x) / zoom).clamp(0.0, pw);
                return Some((0, page_x, 0.0));
            }

            for i in 0..total {
                let &(pw, ph) = st.page_sizes.get(i)?;
                let target_w = pw * zoom;
                let target_h = ph * zoom;
                let offset_x = (widget_width - target_w).max(0.0) / 2.0;
                let page_top = cur_y;
                let page_bot = cur_y + target_h;

                if mouse_y >= page_top && mouse_y <= page_bot + spacing {
                    let page_x = ((mouse_x - offset_x) / zoom).clamp(0.0, pw);
                    let page_y = ((mouse_y - page_top) / zoom).clamp(0.0, ph);
                    return Some((i, page_x, page_y));
                }
                cur_y += target_h + spacing;
            }

            let last_idx = total.saturating_sub(1);
            let &(pw, ph) = st.page_sizes.get(last_idx)?;
            let target_w = pw * zoom;
            let offset_x = (widget_width - target_w).max(0.0) / 2.0;
            let page_x = ((mouse_x - offset_x) / zoom).clamp(0.0, pw);
            Some((last_idx, page_x, ph))
        }
    }
}

fn compute_selection_for_range(
    doc: &Document,
    page_sizes: &[(f64, f64)],
    start_page: usize,
    start_pt: (f64, f64),
    end_page: usize,
    end_pt: (f64, f64),
    style: SelectionStyle,
) -> Option<PdfSelection> {
    let mut highlights = Vec::new();
    let mut text_parts = Vec::new();

    if start_page == end_page {
        let (pw, ph) = *page_sizes.get(start_page)?;
        let page = doc.page(start_page as i32)?;
        let mut rect = Rectangle::new();
        let min_x = start_pt.0.min(end_pt.0).clamp(0.0, pw);
        let min_y = start_pt.1.min(end_pt.1).clamp(0.0, ph);
        let max_x = start_pt.0.max(end_pt.0).clamp(0.0, pw);
        let max_y = start_pt.1.max(end_pt.1).clamp(0.0, ph);

        rect.set_x1(min_x);
        rect.set_y1(min_y);
        rect.set_x2(max_x.max(min_x + 1.0));
        rect.set_y2(max_y.max(min_y + 1.0));

        let mut rects = Vec::new();
        if let Some(region) = page.selected_region(1.0, style, &mut rect) {
            for i in 0..region.num_rectangles() {
                let r = region.rectangle(i);
                rects.push((r.x() as f64, r.y() as f64, r.width() as f64, r.height() as f64));
            }
        }
        let sel_text = page.selected_text(style, &mut rect);
        if let Some(ref t) = sel_text {
            let s = t.as_str().trim_matches('\0').to_string();
            if !s.is_empty() {
                text_parts.push(s);
            }
        }
        if rects.is_empty() && !text_parts.is_empty() {
            rects.push((rect.x1(), rect.y1(), (rect.x2() - rect.x1()).max(4.0), (rect.y2() - rect.y1()).max(4.0)));
        }
        if !rects.is_empty() {
            highlights.push(PageHighlight {
                page_index: start_page,
                rects,
            });
        }
    } else {
        let (p_from, p_to, pt_from, pt_to) = if start_page < end_page {
            (start_page, end_page, start_pt, end_pt)
        } else {
            (end_page, start_page, end_pt, start_pt)
        };

        for p in p_from..=p_to {
            let Some(&(pw, ph)) = page_sizes.get(p) else { continue };
            let Some(page) = doc.page(p as i32) else { continue };
            let mut rect = Rectangle::new();

            if p == p_from {
                rect.set_x1(0.0);
                rect.set_y1(pt_from.1.clamp(0.0, ph));
                rect.set_x2(pw);
                rect.set_y2(ph);
            } else if p == p_to {
                rect.set_x1(0.0);
                rect.set_y1(0.0);
                rect.set_x2(pw);
                rect.set_y2(pt_to.1.clamp(0.0, ph));
            } else {
                rect.set_x1(0.0);
                rect.set_y1(0.0);
                rect.set_x2(pw);
                rect.set_y2(ph);
            }

            let mut rects = Vec::new();
            if let Some(region) = page.selected_region(1.0, style, &mut rect) {
                for i in 0..region.num_rectangles() {
                    let r = region.rectangle(i);
                    rects.push((r.x() as f64, r.y() as f64, r.width() as f64, r.height() as f64));
                }
            }
            if let Some(t) = page.selected_text(style, &mut rect) {
                let s = t.as_str().trim_matches('\0').to_string();
                if !s.is_empty() {
                    text_parts.push(s);
                }
            }
            if rects.is_empty() && !text_parts.is_empty() {
                rects.push((rect.x1(), rect.y1(), (rect.x2() - rect.x1()).max(4.0), (rect.y2() - rect.y1()).max(4.0)));
            }
            if !rects.is_empty() {
                highlights.push(PageHighlight {
                    page_index: p,
                    rects,
                });
            }
        }
    }

    let text = text_parts.join("\n\n");
    if highlights.is_empty() && text.is_empty() {
        None
    } else {
        Some(PdfSelection {
            start_page,
            start_pt,
            end_page,
            end_pt,
            highlights,
            text,
        })
    }
}

fn extract_link_from_mapping(
    doc: &Document,
    page_idx: usize,
    page_h: f64,
    mapping: &poppler::LinkMapping,
) -> Option<PdfLinkItem> {
    let raw: *mut ffi::PopplerLinkMapping = mapping.to_glib_none().0;
    if raw.is_null() {
        return None;
    }
    unsafe {
        let m = raw.as_ref()?;
        // Poppler reports link areas in PDF user space (origin at the BOTTOM-left).
        // The viewer hit-tests in top-left page space, so flip Y here.
        // Add comfortable hit padding so small fonts, punctuation and spaces trigger reliably.
        let pad_x = 2.0;
        let pad_y = 2.5;
        let x1 = m.area.x1.min(m.area.x2) - pad_x;
        let x2 = m.area.x1.max(m.area.x2) + pad_x;
        let y1 = (page_h - m.area.y1.max(m.area.y2) - pad_y).max(0.0);
        let y2 = (page_h - m.area.y1.min(m.area.y2) + pad_y).min(page_h);
        let action = m.action.as_ref()?;

        match action.type_ {
            ffi::POPPLER_ACTION_URI => {
                let uri_ptr = action.uri.uri;
                if !uri_ptr.is_null() {
                    let uri_str = std::ffi::CStr::from_ptr(uri_ptr)
                        .to_string_lossy()
                        .to_string();
                    let tooltip = if uri_str.starts_with("mailto:") {
                        format!("Enviar e-mail para {}", uri_str.trim_start_matches("mailto:"))
                    } else {
                        format!("Abrir link: {}", uri_str)
                    };
                    return Some(PdfLinkItem {
                        page_index: page_idx,
                        rect: (x1, y1, x2, y2),
                        action: PdfLinkAction::Uri(uri_str),
                        tooltip,
                    });
                }
            }
            ffi::POPPLER_ACTION_GOTO_DEST => {
                let goto = action.goto_dest;
                let mut target_page = 0;
                let mut target_top = 0.0;
                let mut target_name = String::new();

                if let Some(dest) = goto.dest.as_ref() {
                    target_page = dest.page_num;
                    target_top = dest.top;
                    if !dest.named_dest.is_null() {
                        target_name = std::ffi::CStr::from_ptr(dest.named_dest)
                            .to_string_lossy()
                            .to_string();
                    }
                } else if !goto.title.is_null() {
                    target_name = std::ffi::CStr::from_ptr(goto.title)
                        .to_string_lossy()
                        .to_string();
                }

                if !target_name.is_empty() {
                    if let Some(resolved) = doc.find_dest(&target_name) {
                        let r_ptr: *mut ffi::PopplerDest = resolved.to_glib_none().0;
                        if let Some(r) = r_ptr.as_ref() {
                            target_page = r.page_num;
                            target_top = r.top;
                        }
                    }
                }

                if target_page > 0 {
                    let page_index = (target_page - 1) as usize;
                    let target_h = doc
                        .page(page_index as i32)
                        .map(|p| p.size().1)
                        .unwrap_or(page_h);
                    let top_offset = if target_top > 0.0 {
                        (target_h - target_top).max(0.0)
                    } else {
                        0.0
                    };
                    let tooltip = if target_name.starts_with("cite.") {
                        let cite_key = target_name
                            .trim_start_matches("cite.0@")
                            .trim_start_matches("cite.");
                        format!("Ir para referência ({})", cite_key)
                    } else if target_name.starts_with("section.")
                        || target_name.starts_with("subsection.")
                    {
                        format!("Ir para seção (página {})", target_page)
                    } else if target_name.starts_with("figure.") {
                        format!("Ir para figura (página {})", target_page)
                    } else if target_name.starts_with("table.") {
                        format!("Ir para tabela (página {})", target_page)
                    } else if !target_name.is_empty() {
                        format!("Ir para: {}", target_name)
                    } else {
                        format!("Ir para página {}", target_page)
                    };

                    return Some(PdfLinkItem {
                        page_index: page_idx,
                        rect: (x1, y1, x2, y2),
                        action: PdfLinkAction::Goto {
                            page_index,
                            top_offset,
                        },
                        tooltip,
                    });
                }
            }
            _ => {}
        }
    }
    None
}

impl Default for PdfViewerWidget {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_links_from_pdf() {
        let pdf_path = Path::new("../../.build/main.pdf");
        if !pdf_path.exists() {
            return;
        }
        let uri = format!("file://{}", pdf_path.canonicalize().unwrap().display());
        let doc = Document::from_file(&uri, None).expect("failed to open doc");
        let n = doc.n_pages() as usize;
        let mut links = Vec::new();
        for i in 0..n {
            if let Some(page) = doc.page(i as i32) {
                let (_, ph) = page.size();
                for m in page.link_mapping() {
                    if let Some(item) = extract_link_from_mapping(&doc, i, ph, &m) {
                        links.push(item);
                    }
                }
            }
        }
        println!("TOTAL EXTRACTED LINKS: {}", links.len());
        let mut uris = 0;
        let mut gotos = 0;
        for l in &links {
            match &l.action {
                PdfLinkAction::Uri(u) => {
                    uris += 1;
                    println!("URI: page={} rect={:?} uri={}", l.page_index, l.rect, u);
                }
                PdfLinkAction::Goto { page_index, top_offset } => {
                    gotos += 1;
                    if gotos <= 5 {
                        println!("GOTO: page={} target_page={} top_off={:.1} tip={}", l.page_index, page_index, top_offset, l.tooltip);
                    }
                }
            }
        }
        println!("URIs: {}, GOTOs: {}", uris, gotos);
        assert!(uris > 0, "should have found URI links");
        assert!(gotos > 0, "should have found GOTO links");
    }

    #[test]
    fn test_text_selection() {
        let pdf_path = Path::new("../../.build/main.pdf");
        if !pdf_path.exists() {
            return;
        }
        let uri = format!("file://{}", pdf_path.canonicalize().unwrap().display());
        let doc = Document::from_file(&uri, None).expect("failed to open doc");
        let page = doc.page(0).expect("failed to get page 0");
        let (pw, _ph) = page.size();
        let mut rect = poppler::Rectangle::new();
        rect.set_x1(0.0);
        rect.set_y1(0.0);
        rect.set_x2(pw);
        rect.set_y2(250.0);
        let text = page.selected_text(poppler::SelectionStyle::Glyph, &mut rect);
        println!("SELECTED TEXT: {:?}", text);
        assert!(text.is_some(), "Text should be found");

        let mut word_rect = poppler::Rectangle::new();
        word_rect.set_x1(90.0);
        word_rect.set_y1(115.0);
        word_rect.set_x2(92.0);
        word_rect.set_y2(117.0);
        let word = page.selected_text(poppler::SelectionStyle::Word, &mut word_rect);
        println!("SELECTED WORD: {:?}", word);

        let mut line_rect = poppler::Rectangle::new();
        line_rect.set_x1(90.0);
        line_rect.set_y1(115.0);
        line_rect.set_x2(92.0);
        line_rect.set_y2(117.0);
        let line = page.selected_text(poppler::SelectionStyle::Line, &mut line_rect);
        println!("SELECTED LINE: {:?}", line);

        let mut multi_rect = poppler::Rectangle::new();
        multi_rect.set_x1(200.0);
        multi_rect.set_y1(115.0);
        multi_rect.set_x2(300.0);
        multi_rect.set_y2(140.0);
        let multi = page.selected_text(poppler::SelectionStyle::Glyph, &mut multi_rect);
        println!("SELECTED MULTI: {:?}", multi);
        if let Some(r) = page.selected_region(1.0, poppler::SelectionStyle::Glyph, &mut multi_rect) {
            println!("MULTI NUM RECTS: {}", r.num_rectangles());
            for i in 0..r.num_rectangles() {
                let rect_int = r.rectangle(i);
                println!("  multi rect {}: x={}, y={}, w={}, h={}", i, rect_int.x(), rect_int.y(), rect_int.width(), rect_int.height());
            }
        }
        let region = page.selected_region(1.0, poppler::SelectionStyle::Glyph, &mut rect);
        println!("REGION IS SOME: {}", region.is_some());
        if let Some(r) = region {
            println!("NUM RECTANGLES: {}", r.num_rectangles());
            for i in 0..r.num_rectangles().min(5) {
                let rect_int = r.rectangle(i);
                println!("  rect {}: x={}, y={}, w={}, h={}", i, rect_int.x(), rect_int.y(), rect_int.width(), rect_int.height());
            }
        }
    }
}
