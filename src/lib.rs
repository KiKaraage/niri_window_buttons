use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::{Arc, LazyLock, Mutex},
};

use futures::StreamExt;
use settings::Settings;
use tracing_subscriber::{EnvFilter, fmt::format::FmtSpan};
use waybar_cffi::{
    Module,
    gtk::{self, Orientation, gio, glib::MainContext, traits::{BoxExt, ContainerExt, StyleContextExt, WidgetExt}},
    waybar_module,
};

mod compositor;
mod errors;
mod global;
mod icons;
mod notifications;
mod screen;
mod settings;
mod system;
mod widget;

use compositor::{WindowInfo, WindowSnapshot};
use errors::ModuleError;
use global::{EventMessage, SharedState};
use notifications::NotificationData;
use system::ProcessInfo;
use widget::WindowButton;

static LOGGING: LazyLock<()> = LazyLock::new(|| {
    if let Err(e) = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_span_events(FmtSpan::CLOSE)
        .try_init()
    {
        eprintln!("tracing subscriber initialization failed: {e}");
    }
});

struct WindowButtonsModule;

impl Module for WindowButtonsModule {
    type Config = Settings;

    fn init(info: &waybar_cffi::InitInfo, settings: Settings) -> Self {
        *LOGGING;

        let shared_state = SharedState::create(settings);
        let context = MainContext::default();

        if let Err(e) = context.block_on(initialize_module(info, shared_state)) {
            tracing::error!(%e, "module initialization failed");
        }

        Self
    }
}

waybar_module!(WindowButtonsModule);

#[tracing::instrument(level = "DEBUG", skip_all, err)]
async fn initialize_module(info: &waybar_cffi::InitInfo, state: SharedState) -> Result<(), ModuleError> {
    let root = info.get_root_widget();
    let button_container = gtk::Box::new(Orientation::Horizontal, 0);
    button_container.style_context().add_class("niri-window-buttons");
    root.add(&button_container);

    let context = MainContext::default();
    context.spawn_local(async move {
        ModuleInstance::create(state, button_container).run_event_loop().await
    });

    Ok(())
}

struct ModuleInstance {
    buttons: BTreeMap<u64, WindowButton>,
    container: gtk::Box,
    previous_snapshot: Option<WindowSnapshot>,
    state: SharedState,
}

impl ModuleInstance {
    fn create(state: SharedState, container: gtk::Box) -> Self {
        Self {
            buttons: BTreeMap::new(),
            container,
            previous_snapshot: None,
            state,
        }
    }

    async fn run_event_loop(&mut self) {
        let display_filter = Arc::new(Mutex::new(self.determine_display_filter().await));

        let mut event_stream = match self.state.create_event_stream() {
            Ok(stream) => Box::pin(stream),
            Err(e) => {
                tracing::error!(%e, "failed to create event stream");
                return;
            }
        };

        while let Some(event) = event_stream.next().await {
            match event {
                EventMessage::Notification(notif) => self.handle_notification(notif).await,
                EventMessage::WindowUpdate(snapshot) => {
                    self.handle_window_update(snapshot, display_filter.clone()).await
                }
                EventMessage::Workspaces(_) => {
                    let updated_filter = self.determine_display_filter().await;
                    *display_filter.lock().expect("display filter lock") = updated_filter;
                }
            }
        }
    }

    #[tracing::instrument(level = "DEBUG", skip(self))]
    async fn determine_display_filter(&self) -> screen::DisplayFilter {
        if self.state.settings().show_all_outputs() {
            return screen::DisplayFilter::ShowAll;
        }

        let compositor = self.state.compositor().clone();
        let available_outputs = match gio::spawn_blocking(move || compositor.query_outputs()).await {
            Ok(Ok(outputs)) => outputs,
            Ok(Err(e)) => {
                tracing::warn!(%e, "failed to query compositor outputs");
                return screen::DisplayFilter::ShowAll;
            }
            Err(_) => {
                tracing::error!("task spawning error");
                return screen::DisplayFilter::ShowAll;
            }
        };

        if available_outputs.len() == 1 {
            return screen::DisplayFilter::ShowAll;
        }

        let Some(gdk_window) = self.container.window() else {
            tracing::warn!("container has no GDK window");
            return screen::DisplayFilter::ShowAll;
        };

        let display = gdk_window.display();
        let Some(monitor) = display.monitor_at_window(&gdk_window) else {
            tracing::warn!(display = ?gdk_window.display(), geometry = ?gdk_window.geometry(), 
                "no monitor found for window");
            return screen::DisplayFilter::ShowAll;
        };

        for (output_name, output_info) in available_outputs.into_iter() {
            let match_result = screen::OutputMatcher::compare(&monitor, &output_info);
            if match_result == screen::OutputMatcher::all() {
                return screen::DisplayFilter::Only(output_name);
            }
        }

        tracing::warn!(?monitor, "no matching compositor output found");
        screen::DisplayFilter::ShowAll
    }

    #[tracing::instrument(level = "TRACE", skip(self))]
    async fn handle_notification(&mut self, notification: Box<NotificationData>) {
        let Some(windows) = &self.previous_snapshot else {
            return;
        };

        if let Some(mut process_id) = notification.get_process_id() {
            tracing::trace!(process_id, "attempting PID-based notification matching");

            let process_map = ProcessWindowMap::build(windows.iter());
            let mut matched = false;

            loop {
                if let Some(window) = process_map.lookup(process_id) {
                    if !window.is_focused {
                        if let Some(button) = self.buttons.get(&window.id) {
                            tracing::trace!(?button, ?window, process_id, 
                                "marking window as urgent via PID match");
                            button.mark_urgent();
                            matched = true;
                        }
                    }
                }

                match ProcessInfo::query(process_id).await {
                    Ok(ProcessInfo { parent_id }) => {
                        if let Some(parent) = parent_id {
                            process_id = parent;
                        } else {
                            break;
                        }
                    }
                    Err(e) => {
                        tracing::info!(process_id, %e, "process tree traversal ended");
                        break;
                    }
                }
            }

            if matched {
                return;
            }
        }

        tracing::trace!("no PID match found for notification");

        if !self.state.settings().notifications_use_desktop_entry() {
            tracing::trace!("desktop entry matching disabled");
            return;
        }

        let Some(desktop_entry) = &notification.get_notification().hints.desktop_entry else {
            tracing::trace!("no desktop entry in notification");
            return;
        };

        let fuzzy_enabled = self.state.settings().notifications_use_fuzzy_matching();
        let mut fuzzy_matches = Vec::new();

        let mapped_entry = self.state.settings()
            .notifications_app_map(desktop_entry)
            .unwrap_or(desktop_entry);
        let entry_lower = mapped_entry.to_lowercase();
        let entry_suffix = mapped_entry.split('.').next_back().unwrap_or_default().to_lowercase();

        let mut exact_match = false;
        for window in windows.iter() {
            let Some(app_identifier) = window.app_id.as_deref() else {
                continue;
            };

            if app_identifier == mapped_entry {
                if let Some(button) = self.buttons.get(&window.id) {
                    tracing::trace!(app_identifier, ?button, ?window, 
                        "exact app ID match for notification");
                    button.mark_urgent();
                    exact_match = true;
                }
            } else if fuzzy_enabled {
                if app_identifier.to_lowercase() == entry_lower {
                    tracing::trace!(app_identifier, ?window, 
                        "case-insensitive app ID match");
                    fuzzy_matches.push(window.id);
                } else if app_identifier.contains('.') {
                    if let Some(suffix) = app_identifier.split('.').next_back() {
                        if suffix.to_lowercase() == entry_suffix {
                            tracing::trace!(app_identifier, ?window, 
                                "suffix-based app ID match");
                            fuzzy_matches.push(window.id);
                        }
                    }
                }
            }
        }

        if !exact_match {
            for window_id in fuzzy_matches {
                if let Some(button) = self.buttons.get(&window_id) {
                    button.mark_urgent();
                }
            }
        }
    }

    #[tracing::instrument(level = "DEBUG", skip(self))]
    async fn handle_window_update(
        &mut self,
        snapshot: WindowSnapshot,
        filter: Arc<Mutex<screen::DisplayFilter>>,
    ) {
        let mut removed_windows = self.buttons.keys().copied().collect::<BTreeSet<_>>();
        let config = self.state.settings();

        for window in snapshot.iter().filter(|w| {
            if !filter.lock().expect("filter lock").should_display(w.get_output().unwrap_or_default()) {
                return false;
            }
            if let Some(app_id) = &w.app_id {
                if config.should_ignore(app_id) {
                    return false;
                }
            }
            true
        }) {
            let button_count = (self.buttons.len() + 1) as i32;
            let min_width = self.state.settings().min_button_width();
            let max_width = self.state.settings().max_button_width();
            let total_limit = self.state.settings().max_taskbar_width();
            
            let optimal_width = if max_width * button_count > total_limit {
                (total_limit / button_count).max(min_width)
            } else {
                max_width
            };

            let button = self.buttons.entry(window.id).or_insert_with(|| {
                let btn = WindowButton::create(&self.state, window);
                btn.get_widget().set_size_request(optimal_width, -1);
                self.container.add(btn.get_widget());
                btn
            });

            button.update_focus(window.is_focused);
            button.update_title(window.title.as_deref());

            removed_windows.remove(&window.id);
            self.container.reorder_child(button.get_widget(), -1);
        }

        for window_id in removed_windows {
            if let Some(button) = self.buttons.remove(&window_id) {
                self.container.remove(button.get_widget());
            }
        }

        if !self.buttons.is_empty() {
            let button_count = self.buttons.len() as i32;
            let min_width = self.state.settings().min_button_width();
            let max_width = self.state.settings().max_button_width();
            let total_limit = self.state.settings().max_taskbar_width();
            
            let final_width = if max_width * button_count > total_limit {
                (total_limit / button_count).max(min_width)
            } else {
                max_width
            };

            for button in self.buttons.values() {
                button.get_widget().set_size_request(final_width, -1);
            }
        }

        self.container.show_all();
        self.previous_snapshot = Some(snapshot);
    }
}

struct ProcessWindowMap<'a>(HashMap<i64, &'a WindowInfo>);

impl<'a> ProcessWindowMap<'a> {
    fn build(windows: impl Iterator<Item = &'a WindowInfo>) -> Self {
        Self(
            windows
                .filter_map(|w| w.pid.map(|pid| (i64::from(pid), w)))
                .collect()
        )
    }

    fn lookup(&self, pid: i64) -> Option<&'a WindowInfo> {
        self.0.get(&pid).copied()
    }
}
