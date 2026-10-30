pub mod apply;
pub mod color;
pub mod contrast;
pub mod derive;
pub mod dirs;
pub mod draft;
pub mod export;
pub mod export_docs;
pub mod import;
pub mod live;
pub mod native;
pub mod palette;
pub mod ramp;
pub mod record;
pub mod reload;
pub mod role;
pub mod slot;
pub mod store;
pub mod target;
pub mod template;
pub mod term;
pub mod wallpaper;
pub mod write;

/// A TOML parse error on one line: where it is, then what it is. The Display
/// form carries a code frame, which does not fit a status line or a toast.
pub(crate) fn toml_message(e: &toml::de::Error) -> String {
    let first = e.to_string().lines().next().unwrap_or_default().to_string();
    if first == e.message() {
        first
    } else {
        format!("{first}: {}", e.message())
    }
}
