using System.Text.Json;

namespace Cliptoo.Core.Services
{
    public static class TextAnalysisUtils
    {
        public static bool IsJson(string input)
        {
            if (string.IsNullOrWhiteSpace(input) || !((input.StartsWith('{') && input.EndsWith('}')) || (input.StartsWith('[') && input.EndsWith(']'))))
            {
                return false;
            }

            try
            {
                JsonDocument.Parse(input);
                return true;
            }
            catch (JsonException)
            {
                return false;
            }
        }

        public static bool IsLikelyXml(string input)
        {
            if (string.IsNullOrWhiteSpace(input)) return false;
            var trimmed = input.Trim();
            return trimmed.StartsWith("<") && trimmed.EndsWith(">");
        }
    }
}