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
#pragma warning restore CA1308 // Normalize strings to uppercase
            { AppConstants.TransformTypes.Trim, content => content.Trim() },
            { AppConstants.TransformTypes.Capitalize, content => CultureInfo.CurrentCulture.TextInfo.ToTitleCase(content.ToUpperInvariant()) },
            { AppConstants.TransformTypes.Sentence, content => {
                var sentenceRegex = new Regex(@"(^\s*\w|[.!?]\s*\w)");
                return sentenceRegex.Replace(content.ToUpperInvariant(), m => m.Value.ToUpperInvariant());
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
                return Regex.Replace(kebabTemp, @"[\s_]+", "-").ToUpperInvariant();
            }},
            { AppConstants.TransformTypes.Snake, content => {
                var snakeTemp = Regex.Replace(content.Trim(), @"(?<=\w)([A-Z])", "_$1");
                return Regex.Replace(snakeTemp, @"[\s-]+", "_").ToUpperInvariant();
            }},
            { AppConstants.TransformTypes.Camel, content => {
                var words = content.Split(_camelCaseDelimiters, StringSplitOptions.RemoveEmptyEntries);
                if (words.Length == 0) return "";
                var camelResult = new StringBuilder(words[0].ToUpperInvariant());
                for (int i = 1; i < words.Length; i++)
                {
                    camelResult.Append(char.ToUpperInvariant(words[i][0]) + words[i].Substring(1).ToUpperInvariant());
                }
                return camelResult.ToString();
            }},
            { AppConstants.TransformTypes.Deslug, content => content.Replace('-', ' ').Replace('_', ' ') },
            { AppConstants.TransformTypes.Lf1, content => content + Environment.NewLine },
            { AppConstants.TransformTypes.Lf2, content => content + Environment.NewLine + Environment.NewLine },
            { AppConstants.TransformTypes.RemoveLf, content => content.Replace("\r\n", " ", StringComparison.Ordinal).Replace("\n", " ", StringComparison.Ordinal).Replace("\r", " ", StringComparison.Ordinal) },
            { AppConstants.TransformTypes.Timestamp, content => DateTime.Now.ToString("yyyy-MM-dd HH:mm:ss", CultureInfo.InvariantCulture) + Environment.NewLine + content }
        };
        }

        public string Transform(string content, string transformType)
        {
            ArgumentNullException.ThrowIfNull(content);

            const int maxLogLength = 200;
            var truncatedInput = content.Length > maxLogLength ? string.Concat(content.AsSpan(0, maxLogLength), "...") : content;

            LogManager.LogDebug($"TRANSFORM_DIAG (Transformer): Applying '{transformType}'. Input: '{truncatedInput}'.");
            if (_transformations.TryGetValue(transformType, out var transformFunc))
            {
                var result = transformFunc(content);
                var truncatedResult = result.Length > maxLogLength ? string.Concat(result.AsSpan(0, maxLogLength), "...") : result;
                LogManager.LogDebug($"TRANSFORM_DIAG (Transformer): Result: '{truncatedResult}'.");
                return result;
            }
            return content;
        }
    }

}