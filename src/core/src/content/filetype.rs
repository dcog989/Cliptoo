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
            Some(e) if DATA.contains(&e) => ClipType::FileData,
            Some(e) if AUDIO.contains(&e) => ClipType::FileAudio,
            Some(e) if DEV.contains(&e) => ClipType::FileDev,
            Some(e) if DOCUMENT.contains(&e) => ClipType::FileDocument,
            Some(e) if crate::image::IMAGE_EXTENSIONS.contains(&e) => ClipType::FileImage,
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
// Database, tabular, statistical, and scientific data files. JSON/YAML/TOML
// config-style formats are deliberately excluded — they stay in DEV because
// their overwhelmingly common use is configuration, not data.
const DATA: &[&str] = &[
    // Databases
    "db", "sqlite", "sqlite3", "mdb", "accdb", "dbf", "dmp", "gpkg", "frm",
    // Tabular / structured
    "csv", "tsv", "parquet", "arrow", "feather", "orc", "jsonl", "ndjson", "geojson",
    // Statistical
    "sav", "dta", "por", "sas7bdat", "rds", "rdata", // Scientific
    "h5", "hdf5", "nc", "mat",
];
// .deb and .rpm are executable package formats; removed from ARCHIVE so DANGER takes precedence.
const DANGER: &[&str] = &[
    "exe", "sh", "bash", "zsh", "fish", "bat", "cmd", "ps1", "apk", "dmg", "run", "so", "elf",
    "bin", "deb", "rpm", "appimage", "msi", "jar", "py", "rb", "pl", "lua",
];
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
    "pdf", "docx", "doc", "xlsx", "xls", "pptx", "ppt", "odt", "ods", "odp", "epub", "rtf",
    "pages", "numbers", "key",
    // Markup / document-format text: markdown and its siblings are document
    // formats (they carry structure, headings, links), distinct from plain
    // text like .txt/.log/.nfo.
    "md", "markdown", "rst", "adoc", "org", "tex",
];

const TEXT_FILE: &[&str] = &["txt", "log", "nfo"];
const VIDEO: &[&str] = &[
    "mp4", "mkv", "avi", "mov", "webm", "flv", "wmv", "m4v", "3gp", "ts", "vob", "ogv", "rm",
    "rmvb",
];

#[cfg(test)]
mod tests {
    use super::FileTypeClassifier;
    use crate::db::models::ClipType;

    fn classify(ext: &str) -> ClipType {
        FileTypeClassifier::classify(std::path::Path::new(ext))
    }

    #[test]
    fn data_files_classify_as_file_data() {
        for ext in [
            "x.db",
            "x.sqlite",
            "x.sqlite3",
            "x.mdb",
            "x.accdb",
            "x.dbf",
            "x.dmp",
            "x.gpkg",
            "x.frm",
            "x.csv",
            "x.tsv",
            "x.parquet",
            "x.arrow",
            "x.feather",
            "x.orc",
            "x.jsonl",
            "x.ndjson",
            "x.geojson",
            "x.sav",
            "x.dta",
            "x.por",
            "x.sas7bdat",
            "x.rds",
            "x.rdata",
            "x.h5",
            "x.hdf5",
            "x.nc",
            "x.mat",
        ] {
            assert_eq!(classify(ext), ClipType::FileData, "for {ext}");
        }
    }

    #[test]
    fn csv_and_tsv_no_longer_classify_as_document_or_text() {
        assert_eq!(classify("x.csv"), ClipType::FileData);
        assert_eq!(classify("x.tsv"), ClipType::FileData);
    }

    #[test]
    fn config_formats_stay_dev() {
        for ext in [
            "x.json", "x.yaml", "x.yml", "x.toml", "x.xml", "x.env", "x.lock",
        ] {
            assert_eq!(classify(ext), ClipType::FileDev, "for {ext}");
        }
    }

    #[test]
    fn neighbours_unchanged() {
        assert_eq!(classify("x.txt"), ClipType::FileText);
        assert_eq!(classify("x.log"), ClipType::FileText);
        assert_eq!(classify("x.nfo"), ClipType::FileText);
        assert_eq!(classify("x.pdf"), ClipType::FileDocument);
        assert_eq!(classify("x.mp3"), ClipType::FileAudio);
        assert_eq!(classify("x.zip"), ClipType::FileArchive);
        assert_eq!(classify("x.xyzzy"), ClipType::FileGeneric);
    }

    #[test]
    fn markup_documents_classify_as_file_document() {
        for ext in ["x.md", "x.markdown", "x.rst", "x.adoc", "x.org", "x.tex"] {
            assert_eq!(classify(ext), ClipType::FileDocument, "for {ext}");
        }
    }
}
