pub mod assemble;
pub mod asset;
pub mod clock;
pub mod constraints;
pub mod core;
pub mod entities;
pub mod fact;
pub mod ident;
pub mod lint;
pub mod loader;
pub mod permissions;
pub mod project;
pub mod provider;
pub mod relations;
pub mod reserved;
pub mod resolve;
pub mod schema;
pub mod season;
pub mod semantics;
pub mod snapshot;
pub mod suggest;
pub mod text;
pub mod types;
pub mod validate;
pub mod yaml_text;
pub use permissions::{PermissionSet, Permissions};
pub use types::*;

/// An I/O failure in the words of a sentence that already names the path —
/// `no such file or directory` — without the platform's ` (os error 2)`.
pub fn io_reason(e: &std::io::Error) -> String {
    let text = e.to_string();
    let text = match text.rfind(" (os error ") {
        Some(at) if text.ends_with(')') => &text[..at],
        _ => &text,
    };
    let mut chars = text.chars();
    chars
        .next()
        .map_or_else(String::new, |c| c.to_lowercase().chain(chars).collect())
}

#[cfg(test)]
mod tests {
    #[test]
    fn io_reason_drops_the_os_error_number() {
        let e = std::fs::read("/definitely/not/here/lute").unwrap_err();
        assert_eq!(super::io_reason(&e), "no such file or directory");
    }
}
