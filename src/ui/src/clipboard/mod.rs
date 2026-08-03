mod listener;
mod reader;

pub use listener::run_listener;

enum ClipboardPayload {
    Text {
        hash: String,
        content: String,
        sup_hash: u64,
    },
    Image {
        hash: String,
        data: Vec<u8>,
        sup_hash: u64,
    },
    FileUri {
        hash: String,
        content: String,
        sup_hash: u64,
    },
}

fn is_blacklisted(source_app: Option<&str>, blacklist: &[String]) -> bool {
    source_app.is_some_and(|app| blacklist.iter().any(|b| app == b || app.ends_with(b)))
}
