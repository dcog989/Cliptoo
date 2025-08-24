using System.IO;

namespace Cliptoo.Core.Services
{
    public class SyntaxHighlighter : ISyntaxHighlighter
    {
        public string? GetHighlightingDefinition(string clipType, string content)
        {
            if (clipType == "file_dev" || clipType.StartsWith("code_"))
            {
                if (IsLikelyXml(content)) return "XML";
                if (IsLikelyJson(content)) return "JavaScript"; // AvalonEdit uses JS for JSON

                // This could be expanded with more sophisticated language detection
                // For now, we'll return a common default for code.
                return "C#";
            }

            return null; // No highlighting for non-code types
        }

        private bool IsLikelyXml(string content)
        {
            var trimmed = content.Trim();
            return trimmed.StartsWith("<") && trimmed.EndsWith(">");
        }

        private bool IsLikelyJson(string content)
        {
            var trimmed = content.Trim();
            return (trimmed.StartsWith("{") && trimmed.EndsWith("}")) || (trimmed.StartsWith("[") && trimmed.EndsWith("]"));
        }
    }
}