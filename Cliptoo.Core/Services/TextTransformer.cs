using System;
using System.Collections.Generic;
using System.Globalization;
using System.Text;
using System.Text.RegularExpressions;
using Cliptoo.Core.Logging;

namespace Cliptoo.Core.Services
{
    public partial class TextTransformer : ITextTransformer
    {
        private readonly Dictionary<string, Func<string, string>> _transformations;
        private static readonly char[] _camelCaseDelimiters = { ' ', '-', '_' };

        // Pre-compiled regex patterns for better performance
        [GeneratedRegex(@"(^\s*\w|[.!?]\s*\w)", RegexOptions.Compiled)]
        private static partial Regex SentenceCaseRegex();

        [GeneratedRegex(@"(?<=\w)([A-Z])", RegexOptions.Compiled)]
        private static partial Regex CamelCaseSplitRegex();

        [GeneratedRegex(@"[\s_]+", RegexOptions.Compiled)]
        private static partial Regex KebabSpaceRegex();

        [GeneratedRegex(@"[\s-]+", RegexOptions.Compiled)]
        private static partial Regex SnakeSpaceRegex();

        public TextTransformer()
        {
            _transformations = new Dictionary<string, Func<string, string>>
            {
                { AppConstants.TransformTypeUpper, content => content.ToUpperInvariant() },
#pragma warning disable CA1308 // Normalize strings to uppercase
                { AppConstants.TransformTypeLower, content => content.ToLowerInvariant() },
#pragma warning restore CA1308 // Normalize strings to uppercase
                { AppConstants.TransformTypeTrim, content => content.Trim() },
                { AppConstants.TransformTypeCapitalize, TransformCapitalize },
                { AppConstants.TransformTypeSentence, TransformSentenceCase },
                { AppConstants.TransformTypeInvert, TransformInvertCase },
                { AppConstants.TransformTypeKebab, TransformKebabCase },
                { AppConstants.TransformTypeSnake, TransformSnakeCase },
                { AppConstants.TransformTypeCamel, TransformCamelCase },
                { AppConstants.TransformTypeDeslug, TransformDeslug },
                { AppConstants.TransformTypeLf1, content => content + Environment.NewLine },
                { AppConstants.TransformTypeLf2, content => content + Environment.NewLine + Environment.NewLine },
                { AppConstants.TransformTypeRemoveLf, TransformRemoveLineFeeds },
                { AppConstants.TransformTypeTimestamp, content => DateTime.Now.ToString("yyyy-MM-dd HH:mm:ss", CultureInfo.InvariantCulture) + Environment.NewLine + content }
            };
        }

        public string Transform(string content, string transformType)
        {
            ArgumentNullException.ThrowIfNull(content);
            ArgumentException.ThrowIfNullOrWhiteSpace(transformType);

            const int maxLogLength = 100;
            var truncatedInput = content.Length > maxLogLength 
                ? string.Concat(content.AsSpan(0, maxLogLength), "...") 
                : content;

            LogManager.LogDebug($"TRANSFORM_DIAG (Transformer): Applying '{transformType}'. Input: '{truncatedInput}'.");
            
            if (_transformations.TryGetValue(transformType, out var transformFunc))
            {
                var result = transformFunc(content);
                var truncatedResult = result.Length > maxLogLength 
                    ? string.Concat(result.AsSpan(0, maxLogLength), "...") 
                    : result;
                LogManager.LogDebug($"TRANSFORM_DIAG (Transformer): Result: '{truncatedResult}'.");
                return result;
            }
            
            return content;
        }

#pragma warning disable CA1308 // Normalize strings to uppercase
        private static string TransformCapitalize(string content)
        {
            return CultureInfo.CurrentCulture.TextInfo.ToTitleCase(content.ToLowerInvariant());
        }

        private static string TransformSentenceCase(string content)
        {
            return SentenceCaseRegex().Replace(content.ToLowerInvariant(), m => m.Value.ToUpperInvariant());
        }
#pragma warning restore CA1308 // Normalize strings to uppercase

        private static string TransformInvertCase(string content)
        {
            var charArray = content.ToCharArray();
            for (int i = 0; i < charArray.Length; i++)
            {
                if (char.IsUpper(charArray[i]))
                    charArray[i] = char.ToLowerInvariant(charArray[i]);
                else if (char.IsLower(charArray[i]))
                    charArray[i] = char.ToUpperInvariant(charArray[i]);
            }
            return new string(charArray);
        }

#pragma warning disable CA1308 // Normalize strings to uppercase
        private static string TransformKebabCase(string content)
        {
            // Convert PascalCase/camelCase to hyphen-separated
            var kebabTemp = CamelCaseSplitRegex().Replace(content.Trim(), "-$1");
            // Replace spaces and underscores with hyphens, then lowercase
            return KebabSpaceRegex().Replace(kebabTemp, "-").ToLowerInvariant();
        }

        private static string TransformSnakeCase(string content)
        {
            // Convert PascalCase/camelCase to underscore-separated
            var snakeTemp = CamelCaseSplitRegex().Replace(content.Trim(), "_$1");
            // Replace spaces and hyphens with underscores, then lowercase
            return SnakeSpaceRegex().Replace(snakeTemp, "_").ToLowerInvariant();
        }
#pragma warning restore CA1308 // Normalize strings to uppercase

        private static string TransformCamelCase(string content)
        {
            var words = content.Split(_camelCaseDelimiters, StringSplitOptions.RemoveEmptyEntries);
            if (words.Length == 0) 
                return string.Empty;

            // First word starts with lowercase
            var camelResult = new StringBuilder();
            if (words[0].Length > 0)
            {
#pragma warning disable CA1308 // Normalize strings to uppercase
                camelResult.Append(char.ToLowerInvariant(words[0][0]));
#pragma warning restore CA1308 // Normalize strings to uppercase
                if (words[0].Length > 1)
                {
#pragma warning disable CA1308 // Normalize strings to uppercase
                    camelResult.Append(words[0].Substring(1).ToLowerInvariant());
#pragma warning restore CA1308 // Normalize strings to uppercase
                }
            }

            // Subsequent words start with uppercase
            for (int i = 1; i < words.Length; i++)
            {
                if (words[i].Length > 0)
                {
                    camelResult.Append(char.ToUpperInvariant(words[i][0]));
                    if (words[i].Length > 1)
                    {
#pragma warning disable CA1308 // Normalize strings to uppercase
                        camelResult.Append(words[i].Substring(1).ToLowerInvariant());
#pragma warning restore CA1308 // Normalize strings to uppercase
                    }
                }
            }

            return camelResult.ToString();
        }

        private static string TransformDeslug(string content)
        {
            // Replace delimiters with spaces and capitalize
            var desluggedContent = content.Replace('-', ' ').Replace('_', ' ');
            return CultureInfo.CurrentCulture.TextInfo.ToTitleCase(desluggedContent);
        }

        private static string TransformRemoveLineFeeds(string content)
        {
            return content
                .Replace("\r\n", " ", StringComparison.Ordinal)
                .Replace("\n", " ", StringComparison.Ordinal)
                .Replace("\r", " ", StringComparison.Ordinal);
        }
    }
}
