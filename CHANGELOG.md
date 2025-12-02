# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2025-12-02

### Added
- Scrollable taskbar with arrow navigation when buttons exceed `max_taskbar_width`
- Configurable scroll arrow glyphs (`scroll_arrow_left` and `scroll_arrow_right`)
- Per-output max taskbar width configuration via `max_taskbar_width_per_output`
- Per-output dimension configuration via `dimensions_per_output` for fine-grained control of button sizes per monitor
- 14 new IPC window management actions:
  - `center-column` - Center the focused column on the screen
  - `center-window` - Center the window on the screen
  - `center-visible-columns` - Center all fully visible columns on the screen
  - `expand-column-to-available-width` - Expand column to fill available width
  - `consume-window-into-column` - Stack window into the adjacent column
  - `expel-window-from-column` - Unstack window from its column
  - `reset-window-height` - Reset window height to default
  - `switch-preset-column-width` - Cycle through preset column widths
  - `switch-preset-window-height` - Cycle through preset window heights
  - `move-window-to-workspace-down` - Move window to workspace below
  - `move-window-to-workspace-up` - Move window to workspace above
  - `move-window-to-monitor-left` - Move window to monitor on the left
  - `move-window-to-monitor-right` - Move window to monitor on the right
  - `toggle-column-tabbed-display` - Toggle tabbed display mode for column

### Changed
- Renamed all click actions to match niri IPC naming conventions for consistency
- Existing actions remain functional but now use standard niri terminology

### Fixed
- Workspace activation is now output-aware for proper multi-monitor support
- Per-output max taskbar width now applies correctly to each monitor
- Arrow visibility updates are deferred to prevent layout corruption
- Multi-monitor setups now properly show active workspace windows simultaneously
- Focus is restored after drag-and-drop to keep viewport at source position

## [0.1.0] - 2024-11-30

Initial release.

### Features
- Window buttons with application icons and optional title text
- Fully configurable click actions (left, right, middle, double-click)
- Configurable context menu
- Per-application click behavior and styling via regex title matching
- Advanced window filtering (by app, title, workspace)
- Drag and drop window reordering
- Dynamic button sizing with taskbar width limits
- Multi-monitor support
- Notification integration with urgency hints
- Custom CSS classes via pattern matching
- Shows active window in Niri overview
