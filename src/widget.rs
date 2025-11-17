use std::{cell::RefCell, fmt::Debug, path::PathBuf, rc::Rc, time::{Duration, Instant}};
use waybar_cffi::gtk::{
    self as gtk, CssProvider, IconLookupFlags, IconSize, IconTheme, Menu, MenuItem, Orientation, ReliefStyle,
    gdk_pixbuf::Pixbuf,
    prelude::{BoxExt, ButtonExt, Cast, ContainerExt, CssProviderExt, DragContextExtManual, GdkPixbufExt, GtkMenuExt, GtkMenuItemExt, IconThemeExt, LabelExt, MenuShellExt, StyleContextExt, WidgetExt, WidgetExtManual},
    DestDefaults, TargetEntry, TargetFlags,
};
use crate::global::SharedState;

pub struct WindowButton {
    app_id: Option<String>,
    gtk_button: gtk::Button,
    layout_box: gtk::Box,
    title_label: gtk::Label,
    display_titles: bool,
    state: SharedState,
    window_id: u64,
}

impl Debug for WindowButton {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WindowButton")
            .field("app_id", &self.app_id)
            .field("display_titles", &self.display_titles)
            .field("window_id", &self.window_id)
            .finish()
    }
}

thread_local! {
    static BUTTON_STYLES: CssProvider = {
        let provider = CssProvider::new();
        if let Err(e) = provider.load_from_data(include_bytes!("styles.css")) {
            tracing::error!(%e, "failed to load CSS");
        }
        provider
    };

    static ICON_THEME_INSTANCE: IconTheme = IconTheme::default().unwrap_or_default();
}

impl WindowButton {
    #[tracing::instrument(level = "TRACE", fields(app_id = &window.app_id))]
    pub fn create(state: &SharedState, window: &niri_ipc::Window) -> Self {
        let state_clone = state.clone();
        let display_titles = state.settings().show_window_titles();

        let icon_gap = state.settings().icon_spacing();
        let layout_box = gtk::Box::new(Orientation::Horizontal, icon_gap);

        let title_label = gtk::Label::new(None);
        title_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        title_label.set_xalign(0.0);

        let gtk_button = gtk::Button::new();
        gtk_button.set_always_show_image(true);
        gtk_button.set_relief(ReliefStyle::None);
        gtk_button.add(&layout_box);

        let max_width = state.settings().max_button_width();
        gtk_button.set_size_request(max_width, -1);

        if display_titles {
            let icon_dim = state.settings().icon_size();
            let max_chars = (max_width - icon_dim - icon_gap - 16) / 8;
            title_label.set_max_width_chars(max_chars);
        }

        BUTTON_STYLES.with(|provider| {
            gtk_button.style_context().add_provider(provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
        });

        let app_id = window.app_id.clone();
        let icon_location = app_id.as_deref().and_then(|id| state_clone.icon_resolver().resolve(id));

        let button = Self {
            app_id,
            gtk_button,
            layout_box,
            title_label,
            display_titles,
            state: state_clone,
            window_id: window.id,
        };

        button.setup_click_handlers(window.id);
        button.setup_right_click_menu(window.id);
        button.setup_drag_reorder();
        button.setup_icon_rendering(icon_location);

        button
    }

    #[tracing::instrument(level = "TRACE")]
    pub fn update_focus(&self, is_focused: bool) {
        let style_ctx = self.gtk_button.style_context();
        if is_focused {
            style_ctx.add_class("focused");
            style_ctx.remove_class("urgent");
        } else {
            style_ctx.remove_class("focused");
        }
        self.gtk_button.queue_draw();
    }

    #[tracing::instrument(level = "TRACE")]
    pub fn update_title(&self, title: Option<&str>) {
        self.gtk_button.set_tooltip_text(title);

        if self.display_titles {
            if let Some(text) = title {
                self.title_label.set_text(text);
                self.title_label.show();
            } else {
                self.title_label.set_text("");
                self.title_label.hide();
            }
        }

        if let Some(app_id) = &self.app_id {
            if let Some(window_title) = title {
                let config = self.state.settings();
                let style_ctx = self.gtk_button.style_context();

                for class in config.get_app_classes(app_id) {
                    style_ctx.remove_class(class);
                }

                for class in config.match_app_rules(app_id, window_title) {
                    style_ctx.add_class(class);
                }
            }
        }
    }

    #[tracing::instrument(level = "TRACE")]
    pub fn mark_urgent(&self) {
        self.gtk_button.style_context().add_class("urgent");
    }

    pub fn get_widget(&self) -> &gtk::Button {
        &self.gtk_button
    }

    fn setup_click_handlers(&self, window_id: u64) {
        let state = self.state.clone();
        let state_middle = self.state.clone();
        let button_ref = self.gtk_button.clone();
        let last_click_time = Rc::new(RefCell::new(Instant::now() - Duration::from_secs(1)));

        self.gtk_button.connect_clicked(move |_| {
            let is_currently_focused = button_ref.style_context().has_class("focused");

            if is_currently_focused && state.settings().click_focused_maximizes() {
                let mut last_click = last_click_time.borrow_mut();
                let now = Instant::now();
                if now.duration_since(*last_click) > Duration::from_millis(300) {
                    *last_click = now;
                    if let Err(e) = state.compositor().maximize_window_column(window_id) {
                        tracing::warn!(%e, id = window_id, "maximize failed");
                    }
                }
            } else {
                if let Err(e) = state.compositor().focus_window(window_id) {
                    tracing::warn!(%e, id = window_id, "focus failed");
                }
            }
        });

        self.gtk_button.connect_button_press_event(move |_, event| {
            if event.button() == 2 {
                if state_middle.settings().middle_click_close() {
                    if let Err(e) = state_middle.compositor().close_window(window_id) {
                        tracing::warn!(%e, id = window_id, "close failed");
                    }
                }
                gtk::glib::Propagation::Stop
            } else {
                gtk::glib::Propagation::Proceed
            }
        });
    }

    #[tracing::instrument(level = "TRACE", skip(self))]
    fn display_context_menu(&self, window_id: u64) {
        let menu = Menu::new();
        menu.set_reserve_toggle_size(false);

        let maximize_item = MenuItem::with_label("  Maximize Column");
        let floating_item = MenuItem::with_label("󰉩  Toggle Floating");
        let close_item = MenuItem::with_label("  Close Window");

        menu.append(&maximize_item);
        menu.append(&floating_item);
        menu.append(&close_item);

        let state_close = self.state.clone();
        close_item.connect_activate(move |_| {
            if let Err(e) = state_close.compositor().close_window(window_id) {
                tracing::warn!(%e, id = window_id, "close via menu failed");
            }
        });

        let state_max = self.state.clone();
        maximize_item.connect_activate(move |_| {
            if let Err(e) = state_max.compositor().maximize_window_column(window_id) {
                tracing::warn!(%e, id = window_id, "maximize via menu failed");
            }
        });

        let state_float = self.state.clone();
        floating_item.connect_activate(move |_| {
            if let Err(e) = state_float.compositor().toggle_floating(window_id) {
                tracing::warn!(%e, id = window_id, "toggle floating failed");
            }
        });

        menu.show_all();
        menu.popup_at_pointer(None);
    }

    fn setup_right_click_menu(&self, window_id: u64) {
        let menu_self = Self {
            app_id: self.app_id.clone(),
            gtk_button: self.gtk_button.clone(),
            layout_box: self.layout_box.clone(),
            title_label: self.title_label.clone(),
            display_titles: self.display_titles,
            state: self.state.clone(),
            window_id,
        };

        self.gtk_button.connect_button_press_event(move |_, event| {
            if event.button() == 3 {
                menu_self.display_context_menu(window_id);
                gtk::glib::Propagation::Stop
            } else {
                gtk::glib::Propagation::Proceed
            }
        });
    }

    fn setup_drag_reorder(&self) {
        tracing::info!("configuring drag-drop for window {}", self.window_id);

        let drag_targets = vec![TargetEntry::new("text/plain", TargetFlags::SAME_APP, 0)];

        self.gtk_button.drag_source_set(
            gtk::gdk::ModifierType::BUTTON1_MASK,
            &drag_targets,
            gtk::gdk::DragAction::MOVE,
        );

        self.gtk_button.drag_dest_set(
            DestDefaults::ALL,
            &drag_targets,
            gtk::gdk::DragAction::MOVE,
        );

        let initial_position = Rc::new(RefCell::new(0));
        let pos_for_begin = initial_position.clone();

        self.gtk_button.connect_drag_begin(move |widget, _| {
            tracing::info!("drag initiated");

            if let Some(parent) = widget.parent() {
                if let Ok(container) = parent.downcast::<gtk::Box>() {
                    let position = container.child_position(widget);
                    *pos_for_begin.borrow_mut() = position;
                    tracing::info!("stored initial position: {}", position);
                }
            }

            widget.style_context().add_class("dragging");
        });

        let window_id = self.window_id;
        self.gtk_button.connect_drag_data_get(move |_, _, data, _, _| {
            data.set_text(&window_id.to_string());
        });

        let button_for_end = self.gtk_button.clone();
        self.gtk_button.connect_drag_end(move |_, _| {
            tracing::info!("drag completed");
            button_for_end.style_context().remove_class("dragging");
        });

        self.gtk_button.connect_drag_motion(move |widget, ctx, _x, _y, _time| {
            if let Some(source) = ctx.drag_get_source_widget() {
                if source != *widget {
                    if let Some(parent) = widget.parent() {
                        if let Ok(container) = parent.downcast::<gtk::Box>() {
                            let source_pos = container.child_position(&source);
                            let target_pos = container.child_position(widget);

                            if source_pos != target_pos {
                                container.reorder_child(&source, target_pos);
                                tracing::trace!("reordered from {} to {}", source_pos, target_pos);
                            }
                        }
                    }
                }
            }
            true
        });

        let button_for_leave = self.gtk_button.clone();
        self.gtk_button.connect_drag_leave(move |_, _, _| {
            button_for_leave.style_context().remove_class("drag-over");
        });

        let state = self.state.clone();
        let pos_for_drop = initial_position.clone();
        self.gtk_button.connect_drag_data_received(move |_widget, ctx, _, _, data, _, time| {
            tracing::info!("drop received");

            if let Some(text) = data.text() {
                if let Ok(dragged_window_id) = text.parse::<u64>() {
                    if let Some(source) = ctx.drag_get_source_widget() {
                        if let Some(parent) = source.parent() {
                            if let Ok(container) = parent.downcast::<gtk::Box>() {
                                let start_pos = *pos_for_drop.borrow();
                                let end_pos = container.child_position(&source);
                                let delta = end_pos - start_pos;

                                tracing::info!("position change: {} -> {} (delta: {})", start_pos, end_pos, delta);

                                match state.compositor().reposition_window(dragged_window_id, delta) {
                                    Ok(()) => {
                                        tracing::info!("reposition successful");
                                        ctx.drag_finish(true, false, time);
                                        return;
                                    }
                                    Err(e) => {
                                        tracing::error!("reposition failed: {}", e);
                                    }
                                }
                            }
                        }
                    }
                }
            }

            ctx.drag_finish(false, false, time);
        });
    }

    #[tracing::instrument(level = "TRACE")]
    fn setup_icon_rendering(&self, icon_path: Option<PathBuf>) {
        let last_allocation = RefCell::new(None);
        let container = self.layout_box.clone();
        let label = self.title_label.clone();
        let show_titles = self.display_titles;
        let icon_dimension = self.state.settings().icon_size();

        self.gtk_button.connect_size_allocate(move |button, allocation| {
            let mut needs_render = container.children().is_empty();

            if !needs_render {
                if let Some(prev_alloc) = last_allocation.take() {
                    if &prev_alloc != allocation {
                        needs_render = true;
                    }
                } else {
                    needs_render = true;
                }

                last_allocation.replace(Some(*allocation));
            }

            if needs_render {
                let dimension = icon_dimension;

                let icon_image = Self::load_icon_image(icon_path.as_ref(), button, dimension)
                    .unwrap_or_else(|| {
                        static FALLBACK: &str = "application-x-executable";

                        ICON_THEME_INSTANCE.with(|theme| {
                            theme.lookup_icon_for_scale(
                                FALLBACK,
                                dimension,
                                button.scale_factor(),
                                IconLookupFlags::empty(),
                            )
                        })
                        .and_then(|info| Self::load_icon_image(info.filename().as_ref(), button, dimension))
                        .unwrap_or_else(|| gtk::Image::from_icon_name(Some(FALLBACK), IconSize::Button))
                    });

                let container_copy = container.clone();
                let label_copy = label.clone();
                let button_copy = button.clone();
                gtk::glib::source::idle_add_local_once(move || {
                    for child in container_copy.children() {
                        container_copy.remove(&child);
                    }

                    container_copy.pack_start(&icon_image, false, false, 0);

                    if show_titles {
                        container_copy.pack_start(&label_copy, true, true, 0);
                    }

                    container_copy.show_all();
                    button_copy.show_all();
                });
            }
        });
    }

    fn load_icon_image(
        path: Option<&PathBuf>,
        button: &gtk::Button,
        size: i32,
    ) -> Option<gtk::Image> {
        let scaled_size = size * button.scale_factor();

        path.and_then(|p| match Pixbuf::from_file_at_scale(p, scaled_size, scaled_size, true) {
            Ok(pixbuf) => Some(pixbuf),
            Err(e) => {
                tracing::info!(%e, ?p, "icon load failed");
                None
            }
        })
        .and_then(|pixbuf| pixbuf.create_surface(0, button.window().as_ref()))
        .map(|surface| gtk::Image::from_surface(Some(&surface)))
    }
}
