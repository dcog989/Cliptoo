use crate::db::models::ClipType;
use std::path::Path;

/// Extension-only file type classifier. Case-insensitive. No magic-byte sniffing.
pub struct FileTypeClassifier;

impl FileTypeClassifier {
    pub fn classify(path: &Path) -> ClipType {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase());

        // Check extensionless dev filenames (Dockerfile, Makefile, etc.) by
        // file_name() when there is no extension.
        if ext.is_none() {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.to_lowercase());
            if let Some(n) = name.as_deref()
                && DEV_FILENAMES.contains(&n)
            {
                return ClipType::FileDev;
            }
        }

        match ext.as_deref() {
            Some(e) if DANGER.contains(&e) => ClipType::FileDanger,
            Some(e) if ARCHIVE.contains(&e) => ClipType::FileArchive,
            Some(e) if AUDIO.contains(&e) => ClipType::FileAudio,
            Some(e) if DATABASE.contains(&e) => ClipType::FileDatabase,
            Some(e) if DEV.contains(&e) => ClipType::FileDev,
            Some(e) if DOCUMENT.contains(&e) => ClipType::FileDocument,
            Some(e) if FONT.contains(&e) => ClipType::FileFont,
            Some(e) if IMAGE.contains(&e) => ClipType::FileImage,
            Some(e) if LINK.contains(&e) => ClipType::FileLink,
            Some(e) if SYSTEM.contains(&e) => ClipType::FileSystem,
            Some(e) if TEXT_FILE.contains(&e) => ClipType::FileText,
            Some(e) if VIDEO.contains(&e) => ClipType::FileVideo,
            _ => ClipType::FileGeneric,
        }
    }
}

const ARCHIVE: &[&str] = &[
    "7z", "zip", "tar", "gz", "bz2", "xz", "rar", "iso", "zst", "lz4", "cab", "ar",
];
const AUDIO: &[&str] = &[
    "mp3", "flac", "wav", "ogg", "aac", "opus", "m4a", "wma", "aiff", "ape",
];
// .deb and .rpm are executable package formats; removed from ARCHIVE so DANGER takes precedence.
const DANGER: &[&str] = &[
    "exe", "sh", "bash", "zsh", "fish", "bat", "cmd", "ps1", "apk", "dmg", "run", "so", "elf",
    "bin", "deb", "rpm", "appimage", "msi", "jar", "py", "rb", "pl", "lua",
];
const DATABASE: &[&str] = &["sqlite", "sqlite3", "db", "sql", "mdb", "accdb", "ldb"];
// Extension-only entries. Extensionless dev filenames (Dockerfile, Makefile, etc.)
// are matched by name in DEV_FILENAMES below.
const DEV: &[&str] = &[
    "rs", "js", "ts", "jsx", "tsx", "go", "cs", "cpp", "c", "h", "hpp", "java", "kt", "swift",
    "json", "yaml", "yml", "toml", "xml", "html", "htm", "css", "scss", "sass", "less", "svelte",
    "vue", "astro", "php", "ex", "exs", "erl", "hs", "ml", "clj", "dart", "r", "jl", "nim", "zig",
    "v", "odin", "d", "f", "f90", "vhd", "sv", "graphql", "gql", "proto", "thrift", "avro",
    "capnp", "lock", "env",
];
// Extensionless filenames matched by file_name() (lowercased), not by extension.
const DEV_FILENAMES: &[&str] = &[
    "dockerfile",
    "makefile",
    "cmakefile",
    "gnumakefile",
    "cargo",
    "gradlefile",
    "tsconfig",
    "jsconfig",
];
const DOCUMENT: &[&str] = &[
    "pdf", "docx", "doc", "xlsx", "xls", "pptx", "ppt", "odt", "ods", "odp", "epub", "csv", "rtf",
    "pages", "numbers", "key",
];
const FONT: &[&str] = &["ttf", "otf", "woff", "woff2", "eot", "pfb", "afm"];
const IMAGE: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "svg", "avif", "heic", "jxl", "ico", "bmp", "tiff", "tif",
    "psd", "xcf", "raw", "arw", "cr2", "nef", "dng",
];
const LINK: &[&str] = &["lnk", "url", "webloc", "desktop"];
const SYSTEM: &[&str] = &[
    "bak", "tmp", "dll", "sys", "cache", "swp", "swo", "pid", "lock", "log",
];
const TEXT_FILE: &[&str] = &[
    "txt", "md", "markdown", "log", "nfo", "rst", "org", "adoc", "tex", "csv", "tsv",
];
const VIDEO: &[&str] = &[
    "mp4", "mkv", "avi", "mov", "webm", "flv", "wmv", "m4v", "3gp", "ts", "vob", "ogv", "rm",
    "rmvb",
];
