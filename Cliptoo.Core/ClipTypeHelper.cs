using System;

namespace Cliptoo.Core
{
    public static class ClipTypeHelper
    {
        public static bool IsFileBased(string clipType)
        {
            return clipType == AppConstants.ClipTypeFolder || clipType.StartsWith("file_", StringComparison.Ordinal);
        }

        public static bool IsLink(string clipType)
        {
            return clipType == AppConstants.ClipTypeLink || clipType == AppConstants.ClipTypeFileLink;
        }

        public static bool IsComparable(string clipType)
        {
            return clipType is AppConstants.ClipTypeText
                or AppConstants.ClipTypeCodeSnippet
                or AppConstants.ClipTypeRtf
                or AppConstants.ClipTypeDev
                or AppConstants.ClipTypeFileText;
        }

        public static bool IsTextual(string clipType)
        {
            return clipType is AppConstants.ClipTypeText
                or AppConstants.ClipTypeRtf
                or AppConstants.ClipTypeCodeSnippet
                or AppConstants.ClipTypeDev
                or AppConstants.ClipTypeFileText
                or AppConstants.ClipTypeLink;
        }

        public static bool IsEditable(string clipType)
        {
            return !IsFileBased(clipType);
        }

        public static bool IsPreviewableAsTextFile(string clipType, string content)
        {
            return (clipType is AppConstants.ClipTypeFileText or AppConstants.ClipTypeDev || (clipType == AppConstants.ClipTypeDocument &&
                    (content.EndsWith(".md", StringComparison.OrdinalIgnoreCase) || content.EndsWith(".markdown", StringComparison.OrdinalIgnoreCase) || content.EndsWith(".csv", StringComparison.OrdinalIgnoreCase))))
                    && !string.Equals(content, Logging.LogManager.LogFilePath, StringComparison.OrdinalIgnoreCase);
        }
    }
}
