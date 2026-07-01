mod listener;
mod reader;
mod refresh;

pub use listener::run_listener;

enum ClipboardPayload {
    Text { hash: String, content: String },
    Image { hash: String, data: Vec<u8> },
    FileUri { hash: String, content: String },
}

fn is_blacklisted(source_app: Option<&str>, blacklist: &[String]) -> bool {
    source_app.is_some_and(|app| blacklist.iter().any(|b| app == b || app.ends_with(b)))
}
