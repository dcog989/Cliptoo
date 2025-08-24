using System;

namespace Cliptoo.Core.Database.Models
{
    public class DbStats
    {
        public long TotalClips { get; set; }
        public long TotalContentLength { get; set; }
        public long PasteCount { get; set; }
        public double DatabaseSizeMb { get; set; }
        public long TotalClipsEver { get; set; }
        public DateTime? CreationTimestamp { get; set; }
        public long PinnedClips { get; set; }
        public DateTime? LastCleanupTimestamp { get; set; }
    }
}