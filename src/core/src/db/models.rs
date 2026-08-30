use serde::{Deserialize, Serialize};

/// All valid ClipType string values stored in the `clips.ClipType` column.
/// Stored as their &str equivalents; see `ClipType::as_str()`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClipType {
    Text,
    /// A text clip whose content is clearly a file path (to a folder or file).
    /// Distinct from `Folder`/`file_*`, which only ever come from files/folders
    /// actually copied via `text/uri-list`.
    FilePath,
    Rtf,
    /// A text clip whose content is an HTML fragment/document, captured from
    /// the `text/html` clipboard MIME type (e.g. a rich-text copy from a
    /// browser or office suite). Stored verbatim so format-preserving pastes
    /// can re-offer `text/html`; previews use the stripped plain text.
    Html,
    Link,
    Color,
    CodeSnippet,
    FileImage,
    FileVideo,
    FileAudio,
    FileArchive,
    FileDocument,
    FileDev,
    FileDanger,
    /// A database / tabular / statistical data file (`.db`, `.csv`,
    /// `.parquet`, `.sav`, …). Distinguished from `FileText`/`FileDocument`
    /// and from `FileDev` (config-style `json`/`yaml`/`toml` stay Dev).
    FileData,
    FileText,
    FileGeneric,
    Folder,
}

impl ClipType {
    /// All variants, in declaration order.
    pub const ALL: [Self; 18] = [
        Self::Text,
        Self::FilePath,
        Self::Rtf,
        Self::Html,
        Self::Link,
        Self::Color,
        Self::CodeSnippet,
        Self::FileImage,
        Self::FileVideo,
        Self::FileAudio,
        Self::FileArchive,
        Self::FileDocument,
        Self::FileDev,
        Self::FileDanger,
        Self::FileData,
        Self::FileText,
        Self::FileGeneric,
        Self::Folder,
    ];

    /// True when the clip represents an on-disk path copied via
    /// `text/uri-list` (a file type or a folder). These are the clips deadhead
    /// detection checks for missing paths; the pure-text clips never reference
    /// disk state.
    pub fn is_file_clip(&self) -> bool {
        matches!(
            self,
            Self::FileImage
                | Self::FileVideo
                | Self::FileAudio
                | Self::FileArchive
                | Self::FileDocument
                | Self::FileDev
                | Self::FileDanger
                | Self::FileData
                | Self::FileText
                | Self::FileGeneric
                | Self::Folder
        )
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::FilePath => "file_path",
            Self::Rtf => "rtf",
            Self::Html => "html",
            Self::Link => "link",
            Self::Color => "color",
            Self::CodeSnippet => "code_snippet",
            Self::FileImage => "file_image",
            Self::FileVideo => "file_video",
            Self::FileAudio => "file_audio",
            Self::FileArchive => "file_archive",
            Self::FileDocument => "file_document",
            Self::FileDev => "file_dev",
            Self::FileDanger => "file_danger",
            Self::FileData => "file_data",
            Self::FileText => "file_text",
            Self::FileGeneric => "file_generic",
            Self::Folder => "folder",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "rtf" => Self::Rtf,
            "html" => Self::Html,
            "file_path" => Self::FilePath,
            "link" => Self::Link,
            "color" => Self::Color,
            "code_snippet" => Self::CodeSnippet,
            "file_image" => Self::FileImage,
            "file_video" => Self::FileVideo,
            "file_audio" => Self::FileAudio,
            "file_archive" => Self::FileArchive,
            "file_document" => Self::FileDocument,
            "file_dev" => Self::FileDev,
            "file_danger" => Self::FileDanger,
            "file_data" => Self::FileData,
            "file_text" => Self::FileText,
            "file_generic" => Self::FileGeneric,
            "folder" => Self::Folder,
            // Removed clip types: db rows rejoin the re-introduced data category;
            // shortcut/system/font files degrade to the generic File
            // classification so existing rows don't render as text.
            "file_database" => Self::FileData,
            "file_font" | "file_link" | "file_system" => Self::FileGeneric,
            _ => Self::Text,
        }
    }
}

/// Lightweight row struct returned to the Slint UI via VecModel<ClipData>.
/// Full `Content` is intentionally omitted; use `PreviewContent` for list display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipData {
    pub id: i64,
    pub preview_content: String,
    pub content_hash: String,
    pub clip_type: ClipType,
    pub source_app: Option<String>,
    pub timestamp: String,
    pub is_bookmarked: bool,
    pub was_trimmed: bool,
    pub has_leading_whitespace: bool,
    pub size_in_bytes: i64,
    pub paste_count: i64,
    pub tags: Option<String>,
    /// Populated at query time from FTS5 snippet(); never stored.
    pub match_context: Option<String>,
    /// Stored in the `IsMultiline` column; set at insert/update time.
    pub is_multiline: bool,
    /// Stored in the `IsDeadhead` column; set by the deadhead maintenance pass.
    /// True when the clip is a file-type path that no longer exists on disk.
    pub is_deadhead: bool,
}
