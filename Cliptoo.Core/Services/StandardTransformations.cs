using System;
using System.Globalization;
using System.Text;
using System.Text.RegularExpressions;

namespace Cliptoo.Core.Services
{
    public abstract partial class RegexTransformation : ITextTransformation
    {
        public abstract string TransformationType { get; }
        public abstract string Transform(string content);

        [GeneratedRegex(@"(^\s*\w|[.!?]\s*\w)", RegexOptions.Compiled)]
        protected static partial Regex SentenceCaseRegex();

        [GeneratedRegex(@"(?<=\w)([A-Z])", RegexOptions.Compiled)]
        protected static partial Regex CamelCaseSplitRegex();

        [GeneratedRegex(@"[\s_]+", RegexOptions.Compiled)]
        protected static partial Regex KebabSpaceRegex();

        [GeneratedRegex(@"[\s-]+", RegexOptions.Compiled)]
        protected static partial Regex SnakeSpaceRegex();
    }

    public class UpperCaseTransformation : ITextTransformation
    {
        public string TransformationType => AppConstants.TransformTypeUpper;
        public string Transform(string content) => content.ToUpperInvariant();
    }

    public class LowerCaseTransformation : ITextTransformation
    {
        public string TransformationType => AppConstants.TransformTypeLower;
        public string Transform(string content) => content.ToLowerInvariant();
    }

    public class TrimTransformation : ITextTransformation
    {
        public string TransformationType => AppConstants.TransformTypeTrim;
        public string Transform(string content) => content.Trim();
    }

    public class CapitalizeTransformation : ITextTransformation
    {
        public string TransformationType => AppConstants.TransformTypeCapitalize;
        public string Transform(string content) => CultureInfo.CurrentCulture.TextInfo.ToTitleCase(content.ToLowerInvariant());
    }

    public class SentenceCaseTransformation : RegexTransformation
    {
        public override string TransformationType => AppConstants.TransformTypeSentence;
        public override string Transform(string content) => SentenceCaseRegex().Replace(content.ToLowerInvariant(), m => m.Value.ToUpperInvariant());
    }

    public class InvertCaseTransformation : ITextTransformation
    {
        public string TransformationType => AppConstants.TransformTypeInvert;
        public string Transform(string content)
        {
            var charArray = content.ToCharArray();
            for (int i = 0; i < charArray.Length; i++)
            {
                if (char.IsUpper(charArray[i])) charArray[i] = char.ToLowerInvariant(charArray[i]);
                else if (char.IsLower(charArray[i])) charArray[i] = char.ToUpperInvariant(charArray[i]);
            }
            return new string(charArray);
        }
    }

    public class KebabCaseTransformation : RegexTransformation
    {
        public override string TransformationType => AppConstants.TransformTypeKebab;
        public override string Transform(string content)
        {
            var temp = CamelCaseSplitRegex().Replace(content.Trim(), "-$1");
            return KebabSpaceRegex().Replace(temp, "-").ToLowerInvariant();
        }
    }

    public class SnakeCaseTransformation : RegexTransformation
    {
        public override string TransformationType => AppConstants.TransformTypeSnake;
        public override string Transform(string content)
        {
            var temp = CamelCaseSplitRegex().Replace(content.Trim(), "_$1");
            return SnakeSpaceRegex().Replace(temp, "_").ToLowerInvariant();
        }
    }

    public class CamelCaseTransformation : ITextTransformation
    {
        public string TransformationType => AppConstants.TransformTypeCamel;
        private static readonly char[] Delimiters = { ' ', '-', '_' };
        public string Transform(string content)
        {
            var words = content.Split(Delimiters, StringSplitOptions.RemoveEmptyEntries);
            if (words.Length == 0) return string.Empty;
            var sb = new StringBuilder();
            sb.Append(words[0].ToLowerInvariant());
            for (int i = 1; i < words.Length; i++)
            {
                if (words[i].Length > 0)
                {
                    sb.Append(char.ToUpperInvariant(words[i][0]));
                    if (words[i].Length > 1) sb.Append(words[i].Substring(1).ToLowerInvariant());
                }
            }
            return sb.ToString();
        }
    }

    public class DeslugTransformation : ITextTransformation
    {
        public string TransformationType => AppConstants.TransformTypeDeslug;
        public string Transform(string content) => CultureInfo.CurrentCulture.TextInfo.ToTitleCase(content.Replace('-', ' ').Replace('_', ' '));
    }

    public class LineFeed1Transformation : ITextTransformation
    {
        public string TransformationType => AppConstants.TransformTypeLf1;
        public string Transform(string content) => content + Environment.NewLine;
    }

    public class LineFeed2Transformation : ITextTransformation
    {
        public string TransformationType => AppConstants.TransformTypeLf2;
        public string Transform(string content) => content + Environment.NewLine + Environment.NewLine;
    }

    public class RemoveLineFeedsTransformation : ITextTransformation
    {
        public string TransformationType => AppConstants.TransformTypeRemoveLf;
        public string Transform(string content) => content.Replace("\r\n", " ", StringComparison.Ordinal).Replace("\n", " ", StringComparison.Ordinal).Replace("\r", " ", StringComparison.Ordinal);
    }

    public class TimestampTransformation : ITextTransformation
    {
        public string TransformationType => AppConstants.TransformTypeTimestamp;
        public string Transform(string content) => DateTime.Now.ToString("yyyy-MM-dd HH:mm:ss", CultureInfo.InvariantCulture) + Environment.NewLine + content;
    }
}
