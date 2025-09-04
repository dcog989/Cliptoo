namespace Cliptoo.Core.Services.Models
{
    /// <summary>
    /// Represents the result of analyzing a piece of clipboard content.
    /// </summary>
    public class ProcessingResult
    {
        /// <summary>
        /// The determined type of the clip (e.g., "text", "link", "color").
        /// </summary>
        public string ClipType { get; }

        /// <summary>
        /// The original content from the clipboard.
        /// </summary>
        public string Content { get; }

        public string? SourceAppOverride { get; }

        public bool SourceHadWhitespaceTrimmed { get; }

        public ProcessingResult(string clipType, string content, bool sourceHadWhitespaceTrimmed = false, string? sourceAppOverride = null)
        {
            ClipType = clipType;
            Content = content;
            SourceHadWhitespaceTrimmed = sourceHadWhitespaceTrimmed;
            SourceAppOverride = sourceAppOverride;
        }
    }
}