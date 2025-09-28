using System;

namespace Cliptoo.Core.Database.Models
{
    public class Clip
    {
        public int Id { get; set; }
        public string? Content { get; set; }
        public string? PreviewContent { get; set; }
        public DateTime Timestamp { get; set; }
        public string ClipType { get; set; } = string.Empty;
        public string? SourceApp { get; set; }
        public bool IsPinned { get; set; }
        public bool WasTrimmed { get; set; }
        public string? MatchContext { get; set; }
        public long SizeInBytes { get; set; }
        public int PasteCount { get; set; }
    }
}