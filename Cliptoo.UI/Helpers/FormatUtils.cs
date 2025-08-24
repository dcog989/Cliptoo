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
                AppConstants.ClipTypes.Text => "Plain Text",
                AppConstants.ClipTypes.Rtf => "Formatted Text",
                AppConstants.ClipTypes.Link => "Web Link",
                AppConstants.ClipTypes.Color => "Color Code",
                AppConstants.ClipTypes.Image => "Image File",
                AppConstants.ClipTypes.Video => "Video File",
                AppConstants.ClipTypes.Audio => "Audio File",
                AppConstants.ClipTypes.Archive => "Archive File",
                AppConstants.ClipTypes.Document => "Document File",
                AppConstants.ClipTypes.Dev => "Dev File",
                AppConstants.ClipTypes.CodeSnippet => "Code Snippet",
                AppConstants.ClipTypes.Folder => "Folder",
                AppConstants.ClipTypes.Danger => "Potentially Unsafe File",
                AppConstants.ClipTypes.FileText => "Text File",
                AppConstants.ClipTypes.Generic => "Generic File",
                AppConstants.ClipTypes.Database => "Database File",
                AppConstants.ClipTypes.Font => "Font File",
                AppConstants.ClipTypes.FileLink => "Link File",
                AppConstants.ClipTypes.System => "System File",
                _ => "Clip"
            };
        }
    }
}