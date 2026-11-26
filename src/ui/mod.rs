pub mod components;
pub mod icons;
pub mod knowledge;
pub mod navigation;
pub mod preferences;
pub mod views;
pub mod quick_actions;
pub mod results;
pub mod rtt_view;
pub mod sidebar;
pub mod theme;
pub mod workspace;

/// Возвращает имя клавиши модификатора команд в зависимости от целевой ОС (Cmd для macOS, Ctrl для Linux/Windows).
pub fn cmd_key_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "Cmd"
    } else {
        "Ctrl"
    }
}
