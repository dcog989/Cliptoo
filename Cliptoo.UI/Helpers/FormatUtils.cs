using Cliptoo.Core;

namespace Cliptoo.UI.Helpers
{
    public static class FormatUtils
    {
        public static string FormatBytes(long bytes)
        {
            var suf = new[] { "B", "KB", "MB", "GB", "TB", "PB", "EB" };
            if (bytes == 0)
                return "0 " + suf[0];
            long absoluteBytes = Math.Abs(bytes);
            int place = Convert.ToInt32(Math.Floor(Math.Log(absoluteBytes, 1024)));
            double num = Math.Round(absoluteBytes / Math.Pow(1024, place));
            return (Math.Sign(bytes) * num) + " " + suf[place];
        }

        public static string GetFriendlyClipTypeName(string clipType)
        {
            return clipType switch
            {
                AppConstants.ClipTypeText => "Plain Text",
                AppConstants.ClipTypeRtf => "Formatted Text",
                AppConstants.ClipTypeLink => "Web Link",
                AppConstants.ClipTypeColor => "Color Code",
                AppConstants.ClipTypeImage => "Image File",
                AppConstants.ClipTypeVideo => "Video File",
                AppConstants.ClipTypeAudio => "Audio File",
                AppConstants.ClipTypeArchive => "Archive File",
                AppConstants.ClipTypeDocument => "Document File",
                AppConstants.ClipTypeDev => "Dev File",
                AppConstants.ClipTypeCodeSnippet => "Code Snippet",
                AppConstants.ClipTypeFolder => "Folder",
                AppConstants.ClipTypeDanger => "Potentially Unsafe File",
                AppConstants.ClipTypeFileText => "Text File",
                AppConstants.ClipTypeGeneric => "Generic File",
                AppConstants.ClipTypeDatabase => "Database File",
                AppConstants.ClipTypeFont => "Font File",
                AppConstants.ClipTypeFileLink => "Link File",
                AppConstants.ClipTypeSystem => "System File",
                _ => "Clip"
            };
        }
    }
}