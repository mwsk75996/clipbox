//! Shared application logic for Clipbox.
//!
//! Keep this crate independent from Tauri and GTK so both desktop frontends
//! can use the same core as the project grows.

mod storage;

pub use storage::ClipboardStore;

/// The application name shared by frontends.
pub const APP_NAME: &str = "Clipbox";

#[cfg(test)]
mod tests {
    use super::APP_NAME;

    #[test]
    fn exposes_the_application_name() {
        assert_eq!(APP_NAME, "Clipbox");
    }
}
