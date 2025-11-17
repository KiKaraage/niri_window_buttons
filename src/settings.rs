use std::collections::HashMap;
use itertools::Itertools;
use regex::Regex;
use serde::{Deserialize, Deserializer};

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Settings {
    #[serde(default)]
    apps: HashMap<String, Vec<AppRule>>,
    #[serde(default)]
    notifications: NotificationConfig,
    #[serde(default)]
    show_all_outputs: bool,
    #[serde(default)]
    only_current_workspace: bool,
    #[serde(default)]
    show_window_titles: bool,
    #[serde(default = "default_min_width")]
    min_button_width: i32,
    #[serde(default = "default_max_width")]
    max_button_width: i32,
    #[serde(default = "default_icon_size")]
    icon_size: i32,
    #[serde(default = "default_spacing")]
    icon_spacing: i32,
    #[serde(default = "default_max_taskbar")]
    max_taskbar_width: i32,
    #[serde(default = "default_true")]
    middle_click_close: bool,
    #[serde(default = "default_true")]
    click_focused_maximizes: bool,
    #[serde(default)]
    ignore_app_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NotificationConfig {
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default)]
    map_app_ids: HashMap<String, String>,
    #[serde(default = "default_true")]
    use_desktop_entry: bool,
    #[serde(default)]
    use_fuzzy_matching: bool,
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            map_app_ids: HashMap::new(),
            use_desktop_entry: true,
            use_fuzzy_matching: false,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct AppRule {
    #[serde(rename = "match", deserialize_with = "parse_regex")]
    pattern: Regex,
    class: String,
}

fn parse_regex<'de, D>(deserializer: D) -> Result<Regex, D::Error>
where
    D: Deserializer<'de>,
{
    let pattern = String::deserialize(deserializer)?;
    Regex::new(&pattern).map_err(serde::de::Error::custom)
}

fn default_true() -> bool { true }
fn default_min_width() -> i32 { 150 }
fn default_max_width() -> i32 { 235 }
fn default_icon_size() -> i32 { 24 }
fn default_spacing() -> i32 { 6 }
fn default_max_taskbar() -> i32 { 1200 }

impl Settings {
    pub fn get_app_classes(&self, app_id: &str) -> Vec<&str> {
        self.apps
            .get(app_id)
            .map(|rules| rules.iter().map(|r| r.class.as_str()).collect_vec())
            .unwrap_or_default()
    }

    pub fn match_app_rules<'a>(
        &'a self,
        app_id: &str,
        title: &'a str,
    ) -> Box<dyn Iterator<Item = &'a str> + 'a> {
        match self.apps.get(app_id) {
            Some(rules) => Box::new(
                rules
                    .iter()
                    .filter(move |rule| rule.pattern.is_match(title))
                    .map(|rule| rule.class.as_str())
            ),
            None => Box::new(std::iter::empty()),
        }
    }

    pub fn notifications_enabled(&self) -> bool {
        self.notifications.enabled
    }

    pub fn notifications_app_map(&self, app_id: &str) -> Option<&str> {
        self.notifications.map_app_ids.get(app_id).map(String::as_str)
    }

    pub fn notifications_use_desktop_entry(&self) -> bool {
        self.notifications.use_desktop_entry
    }

    pub fn notifications_use_fuzzy_matching(&self) -> bool {
        self.notifications.use_fuzzy_matching
    }

    pub fn show_all_outputs(&self) -> bool {
        self.show_all_outputs
    }

    pub fn only_current_workspace(&self) -> bool {
        self.only_current_workspace
    }

    pub fn show_window_titles(&self) -> bool {
        self.show_window_titles
    }

    pub fn min_button_width(&self) -> i32 {
        self.min_button_width
    }

    pub fn max_button_width(&self) -> i32 {
        self.max_button_width
    }

    pub fn icon_size(&self) -> i32 {
        self.icon_size
    }

    pub fn icon_spacing(&self) -> i32 {
        self.icon_spacing
    }

    pub fn should_ignore(&self, app_id: &str) -> bool {
        self.ignore_app_ids.iter().any(|id| id == app_id)
    }

    pub fn max_taskbar_width(&self) -> i32 {
        self.max_taskbar_width
    }

    pub fn middle_click_close(&self) -> bool {
        self.middle_click_close
    }

    pub fn click_focused_maximizes(&self) -> bool {
        self.click_focused_maximizes
    }
}
