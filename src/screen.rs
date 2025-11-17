use niri_ipc::{LogicalOutput, Output};
use waybar_cffi::gtk::gdk::{Monitor, traits::MonitorExt};

#[derive(Debug, Clone)]
pub enum DisplayFilter {
    ShowAll,
    Only(String),
}

impl DisplayFilter {
    pub fn should_display(&self, output: &str) -> bool {
        match self {
            Self::ShowAll => true,
            Self::Only(name) => name == output,
        }
    }
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct OutputMatcher: u8 {
        const GEOMETRY = 1 << 0;
        const MODEL = 1 << 1;
        const MANUFACTURER = 1 << 2;
    }
}

impl OutputMatcher {
    pub fn compare(monitor: &Monitor, output: &Output) -> Self {
        let Some(logical_output) = &output.logical else {
            tracing::info!(name = output.name, "output missing logical configuration");
            return Self::empty();
        };

        let mut result = Self::empty();

        result.set(
            OutputMatcher::GEOMETRY,
            MonitorGeometry::from_gdk(monitor) == MonitorGeometry::from_niri(logical_output),
        );

        result.set(
            OutputMatcher::MODEL,
            match (monitor.model(), &output.model) {
                (Some(gdk_model), niri_model) => gdk_model.as_str() == niri_model,
                (None, niri_model) if niri_model.is_empty() => true,
                _ => false,
            },
        );

        result.set(
            OutputMatcher::MANUFACTURER,
            match (monitor.manufacturer(), &output.make) {
                (Some(gdk_make), niri_make) => gdk_make.as_str() == niri_make,
                (None, niri_make) if niri_make.is_empty() => true,
                _ => false,
            },
        );

        result
    }
}

#[derive(Debug, Clone, Copy)]
struct MonitorGeometry {
    width: i32,
    height: i32,
    x: i32,
    y: i32,
}

impl MonitorGeometry {
    fn from_gdk(monitor: &Monitor) -> Self {
        let geom = monitor.geometry();
        let scale = monitor.scale_factor();

        Self {
            width: geom.width() * scale,
            height: geom.height() * scale,
            x: geom.x() * scale,
            y: geom.y() * scale,
        }
    }

    fn from_niri(logical: &LogicalOutput) -> Self {
        let scale = logical.scale.ceil() as i32;

        Self {
            width: (logical.width as i32) * scale,
            height: (logical.height as i32) * scale,
            x: logical.x * scale,
            y: logical.y * scale,
        }
    }
}

impl PartialEq for MonitorGeometry {
    fn eq(&self, other: &Self) -> bool {
        let width_ratio = (self.width as f64) / (other.width as f64);
        let height_ratio = (self.height as f64) / (other.height as f64);

        let width_diff = (width_ratio - 1.0).abs();
        let height_diff = (height_ratio - 1.0).abs();

        width_diff < 0.03 && height_diff < 0.03 && self.x == other.x && self.y == other.y
    }
}
