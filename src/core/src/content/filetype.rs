use crate::db::models::ClipType;
use std::io::Read;
use std::path::Path;

/// File type classifier. Extension lists first, case-insensitive; files that
/// match no list are sniffed for text content.
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
            // Unknown or missing extension: the name says nothing, so read a
            // header. Extensionless text (LICENSE, README, notes) is common
            // enough that guessing binary here is wrong.
            _ if Self::sniffs_as_text(path) => ClipType::FileText,
            _ => ClipType::FileGeneric,
        }
    }

    /// Whether the first `TEXT_SNIFF_BYTES` of `path` look like text. A NUL
    /// byte means binary; unreadable or missing files are not text.
    fn sniffs_as_text(path: &Path) -> bool {
        let Ok(file) = std::fs::File::open(path) else {
            return false;
        };
        let mut buf = Vec::with_capacity(TEXT_SNIFF_BYTES);
        if file
            .take(TEXT_SNIFF_BYTES as u64)
            .read_to_end(&mut buf)
            .is_err()
        {
            return false;
        }
        !buf.contains(&0)
    }
}

const TEXT_SNIFF_BYTES: usize = 8192;

const ARCHIVE: &[&str] = &[
    "7z", "zip", "tar", "gz", "tgz", "tbz", "tbz2", "txz", "tlz", "tzst", "bz2", "xz", "lz",
    "lzma", "lz4", "zst", "br", "rar", "iso", "cab", "ar", "cpio", "squashfs", "aab",
];
const AUDIO: &[&str] = &[
    "mp3", "flac", "wav", "ogg", "oga", "aac", "opus", "m4a", "m4b", "wma", "aiff", "aif", "aifc",
    "au", "snd", "mid", "midi", "amr", "ac3", "caf", "wv", "mka", "spx", "ape",
];
// Database, tabular, statistical, and scientific data files. JSON/YAML/TOML
// config-style formats are deliberately excluded — they stay in DEV because
// their overwhelmingly common use is configuration, not data.
const DATA: &[&str] = &[
    // Databases
    "db", "db3", "sqlite", "sqlite3", "mdb", "accdb", "dbf", "dmp", "gpkg", "frm",
    // Tabular / structured
    "csv", "tsv", "parquet", "arrow", "feather", "orc", "jsonl", "ndjson", "geojson",
    // Statistical
    "sav", "dta", "por", "xpt", "sas7bdat", "rds", "rdata", // Scientific
    "h5", "hdf", "hdf5", "nc", "mat", "sdf",
];
// .deb and .rpm are executable package formats; removed from ARCHIVE so DANGER takes precedence.
const DANGER: &[&str] = &[
    "exe", "sh", "bash", "zsh", "fish", "bat", "cmd", "ps1", "apk", "dmg", "run", "so", "elf",
    "bin", "deb", "rpm", "appimage", "msi", "msix", "jar", "py", "rb", "pl", "lua",
    // Windows / macOS executable and library formats.
    "dll", "dylib", "sys", "com", "scr", "pif", "cpl", "lnk", "hta", "reg", "vbs", "vbe", "wsf",
    "wsh", "jse",
];
// Extension-only entries. Extensionless dev filenames (Dockerfile, Makefile, etc.)
// are matched by name in DEV_FILENAMES below.
// `ts` is both TypeScript source and an MPEG transport stream. The dev reading
// wins (it is far more common on a desktop clipboard), so it appears here only.
const DEV: &[&str] = &[
    "rs",
    "js",
    "mjs",
    "cjs",
    "ts",
    "mts",
    "cts",
    "jsx",
    "tsx",
    "go",
    "cs",
    "cpp",
    "cc",
    "cxx",
    "c++",
    "c",
    "h",
    "hpp",
    "hh",
    "hxx",
    "ipp",
    "tpp",
    "inl",
    "java",
    "kt",
    "kts",
    "scala",
    "sc",
    "groovy",
    "gradle",
    "swift",
    "m",
    "mm",
    "fs",
    "fsx",
    "fsi",
    "vb",
    "pas",
    "pp",
    "ex",
    "exs",
    "erl",
    "hs",
    "ml",
    "clj",
    "rkt",
    "scm",
    "lisp",
    "el",
    "dart",
    "r",
    "jl",
    "nim",
    "zig",
    "v",
    "odin",
    "d",
    "f",
    "f90",
    "asm",
    "s",
    "vhd",
    "sv",
    "sol",
    "cairo",
    "move",
    "qml",
    "tcl",
    "awk",
    "coffee",
    "elm",
    // Templates: HTML/Python/JS/Ruby/PHP-style templating, all source files.
    "tpl",
    "tmpl",
    "jinja",
    "jinja2",
    "j2",
    "hbs",
    "handlebars",
    "mustache",
    "ejs",
    "pug",
    "jade",
    "haml",
    "slim",
    "liquid",
    "twig",
    "njk",
    "erb",
    "mako",
    "eex",
    "heex",
    "leex",
    "razor",
    "cshtml",
    // Shaders.
    "glsl",
    "vert",
    "frag",
    "comp",
    "hlsl",
    "wgsl",
    "cu",
    "cuh",
    // Web, markup, config and build files.
    "json",
    "jsonc",
    "json5",
    "yaml",
    "yml",
    "toml",
    "ini",
    "cfg",
    "conf",
    "properties",
    "env",
    "lock",
    "plist",
    "desktop",
    "service",
    "xml",
    "xaml",
    "html",
    "htm",
    "css",
    "scss",
    "sass",
    "less",
    "svelte",
    "vue",
    "astro",
    "php",
    "graphql",
    "gql",
    "proto",
    "thrift",
    "avro",
    "capnp",
    "sql",
    "tf",
    "tfvars",
    "hcl",
    "nix",
    "cmake",
    "mk",
    "diff",
    "patch",
    "ipynb",
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
    "justfile",
    "rakefile",
    "gemfile",
    "podfile",
    "procfile",
    "vagrantfile",
    "jenkinsfile",
    "brewfile",
];
const DOCUMENT: &[&str] = &[
    "pdf", "docx", "docm", "doc", "dotx", "xlsx", "xlsm", "xlsb", "xls", "pptx", "pptm", "ppt",
    "odt", "ods", "odp", "odg", "odf", "epub", "mobi", "azw3", "djvu", "rtf", "wpd", "pages",
    "numbers", "key",
    // Markup / document-format text: markdown and its siblings are document
    // formats (they carry structure, headings, links), distinct from plain
    // text like .txt/.log/.nfo.
    "md", "markdown", "rst", "adoc", "org", "tex", "bib", "typ", "qmd",
];

const TEXT_FILE: &[&str] = &["txt", "text", "log", "nfo"];
const VIDEO: &[&str] = &[
    "mp4", "mkv", "avi", "mov", "webm", "flv", "wmv", "m4v", "mpg", "mpeg", "m2ts", "3gp", "3g2",
    "vob", "ogv", "ogm", "asf", "f4v", "divx", "wtv", "rm", "rmvb",
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

    #[test]
    fn unknown_extension_is_sniffed_for_text() {
        let dir = std::env::temp_dir().join(format!("cliptoo-sniff-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let write = |name: &str, bytes: &[u8]| {
            let p = dir.join(name);
            std::fs::write(&p, bytes).unwrap();
            p
        };

        let text = write("LICENSE", b"GNU GENERAL PUBLIC LICENSE\nVersion 3\n");
        assert_eq!(FileTypeClassifier::classify(&text), ClipType::FileText);
        let extless = write("notes", "café ☕\n".as_bytes());
        assert_eq!(FileTypeClassifier::classify(&extless), ClipType::FileText);

        let binary = write("blob", b"\x7fELF\x02\x01\x01\x00\x00\x00");
        assert_eq!(FileTypeClassifier::classify(&binary), ClipType::FileGeneric);

        // Missing path: nothing to sniff, stays generic.
        assert_eq!(
            FileTypeClassifier::classify(&dir.join("gone")),
            ClipType::FileGeneric
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn templates_and_modern_languages_are_dev() {
        for ext in [
            "x.tpl",
            "x.tmpl",
            "x.jinja",
            "x.jinja2",
            "x.j2",
            "x.hbs",
            "x.mustache",
            "x.ejs",
            "x.liquid",
            "x.twig",
            "x.erb",
            "x.heex",
            "x.mjs",
            "x.cts",
            "x.scala",
            "x.groovy",
            "x.fs",
            "x.sol",
            "x.glsl",
            "x.cu",
            "x.sql",
            "x.tf",
            "x.nix",
            "x.ini",
            "x.diff",
            "x.ipynb",
        ] {
            assert_eq!(classify(ext), ClipType::FileDev, "for {ext}");
        }
    }

    #[test]
    fn ts_resolves_to_dev_not_video() {
        // `.ts` is also an MPEG transport stream. The dev reading wins, and the
        // list must not claim both.
        assert_eq!(classify("x.ts"), ClipType::FileDev);
    }
}
