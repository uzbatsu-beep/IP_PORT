pub mod results;
pub mod rtt_view;
pub mod sidebar;
pub mod theme;

/// Возвращает имя клавиши модификатора команд в зависимости от целевой ОС (Cmd для macOS, Ctrl для Linux/Windows).
pub fn cmd_key_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "Cmd"
    } else {
        "Ctrl"
    }
}
