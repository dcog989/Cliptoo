use serde::{Deserialize, Serialize};

/// All valid ClipType string values stored in the `clips.ClipType` column.
/// Stored as their &str equivalents; see `ClipType::as_str()`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClipType {
    Text,
    Rtf,
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
    FileText,
    FileGeneric,
    FileDatabase,
    FileFont,
    FileLink,
    FileSystem,
    Folder,
}

impl ClipType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Rtf => "rtf",
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
            Self::FileText => "file_text",
            Self::FileGeneric => "file_generic",
            Self::FileDatabase => "file_database",
            Self::FileFont => "file_font",
            Self::FileLink => "file_link",
            Self::FileSystem => "file_system",
            Self::Folder => "folder",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "rtf" => Self::Rtf,
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
            "file_text" => Self::FileText,
            "file_generic" => Self::FileGeneric,
            "file_database" => Self::FileDatabase,
            "file_font" => Self::FileFont,
            "file_link" => Self::FileLink,
            "file_system" => Self::FileSystem,
            "folder" => Self::Folder,
            _ => Self::Text,
        }
    }

    /// Returns true if this type represents a file-system path clip.
    pub fn is_file_type(&self) -> bool {
        matches!(
            self,
            Self::FileImage
                | Self::FileVideo
                | Self::FileAudio
                | Self::FileArchive
                | Self::FileDocument
                | Self::FileDev
                | Self::FileDanger
                | Self::FileText
                | Self::FileGeneric
                | Self::FileDatabase
                | Self::FileFont
                | Self::FileLink
                | Self::FileSystem
                | Self::Folder
        )
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
