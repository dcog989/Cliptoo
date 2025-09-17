using System;
using System.Collections.Generic;
using System.Globalization;
using System.Text;
using System.Text.RegularExpressions;
using Cliptoo.Core.Logging;

namespace Cliptoo.Core.Services
{
    public class TextTransformer : ITextTransformer
    {
        private readonly Dictionary<string, Func<string, string>> _transformations;
        private static readonly char[] _camelCaseDelimiters = { ' ', '-', '_' };

        public TextTransformer()
        {
            _transformations = new Dictionary<string, Func<string, string>>
        {
            { AppConstants.TransformTypes.Upper, content => content.ToUpperInvariant() },
#pragma warning disable CA1308 // Normalize strings to uppercase
            { AppConstants.TransformTypes.Lower, content => content.ToLowerInvariant() },
            { AppConstants.TransformTypes.Trim, content => content.Trim() },
            { AppConstants.TransformTypes.Capitalize, content => CultureInfo.CurrentCulture.TextInfo.ToTitleCase(content.ToLowerInvariant()) },
            { AppConstants.TransformTypes.Sentence, content => {
                var sentenceRegex = new Regex(@"(^\s*\w|[.!?]\s*\w)");
                return sentenceRegex.Replace(content.ToLowerInvariant(), m => m.Value.ToUpperInvariant());
            }},
            { AppConstants.TransformTypes.Invert, content => {
                var charArray = content.ToCharArray();
                for (int i = 0; i < charArray.Length; i++)
                {
                    if (char.IsUpper(charArray[i]))
                        charArray[i] = char.ToLowerInvariant(charArray[i]);
                    else if (char.IsLower(charArray[i]))
                        charArray[i] = char.ToUpperInvariant(charArray[i]);
                }
                return new string(charArray);
            }},
            { AppConstants.TransformTypes.Kebab, content => {
                var kebabTemp = Regex.Replace(content.Trim(), @"(?<=\w)([A-Z])", "-$1");
                return Regex.Replace(kebabTemp, @"[\s_]+", "-").ToLowerInvariant();
            }},
            { AppConstants.TransformTypes.Snake, content => {
                var snakeTemp = Regex.Replace(content.Trim(), @"(?<=\w)([A-Z])", "_$1");
                return Regex.Replace(snakeTemp, @"[\s-]+", "_").ToLowerInvariant();
            }},
            { AppConstants.TransformTypes.Camel, content => {
                var words = content.Split(_camelCaseDelimiters, StringSplitOptions.RemoveEmptyEntries);
                if (words.Length == 0) return "";
                var camelResult = new StringBuilder(words[0].ToLowerInvariant());
                for (int i = 1; i < words.Length; i++)
                {
                    camelResult.Append(char.ToUpperInvariant(words[i][0]) + words[i].Substring(1).ToLowerInvariant());
                }
                return camelResult.ToString();
            }},
#pragma warning restore CA1308 // Normalize strings to uppercase
            { AppConstants.TransformTypes.Deslug, content => content.Replace('-', ' ').Replace('_', ' ') },
            { AppConstants.TransformTypes.Lf1, content => content + Environment.NewLine },
            { AppConstants.TransformTypes.Lf2, content => content + Environment.NewLine + Environment.NewLine },
            { AppConstants.TransformTypes.RemoveLf, content => content.Replace("\r\n", " ", StringComparison.Ordinal).Replace("\n", " ", StringComparison.Ordinal).Replace("\r", " ", StringComparison.Ordinal) },
            { AppConstants.TransformTypes.Timestamp, content => DateTime.Now.ToString("yyyy-MM-dd HH:mm:ss", CultureInfo.InvariantCulture) + Environment.NewLine + content }
        };
        }

        public string Transform(string content, string transformType)
        {
            LogManager.LogDebug($"TRANSFORM_DIAG (Transformer): Applying '{transformType}'. Input: '{content}'.");
            if (_transformations.TryGetValue(transformType, out var transformFunc))
            {
                var result = transformFunc(content);
                LogManager.LogDebug($"TRANSFORM_DIAG (Transformer): Result: '{result}'.");
                return result;
            }
            return content;
        }
    }
}