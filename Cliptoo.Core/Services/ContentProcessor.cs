using System;
using System.Buffers;
using System.Collections.Generic;
using System.Linq;
using Cliptoo.Core.Services.Models;
using System.IO;

namespace Cliptoo.Core.Services
{
    public class ContentProcessor : IContentProcessor
    {
        private readonly IFileTypeClassifier _fileTypeClassifier;
        private static readonly SearchValues<char> _invalidColorChars = SearchValues.Create("\n\r<>[]");

        private static readonly HashSet<string> _codeKeywords = new(StringComparer.Ordinal)
        {
            "public", "private", "protected", "static", "void", "class", "interface", "enum", "if", "else", "switch", "case", "for", "foreach", "while", "do", "return", "break", "using", "namespace", "import", "from", "new", "var", "let", "const", "get", "set", "async", "await", "yield", "try", "catch", "finally", "throw", "int", "string", "bool", "double", "float", "char", "object", "def", "lambda", "import", "from", "as", "with", "function", "return", "new", "var", "let", "const", "=>", "SELECT", "FROM", "WHERE", "GROUP", "BY", "ORDER", "JOIN", "INSERT", "UPDATE", "DELETE", "<html>", "<div>", "<script>", "<style>", "<body>"
        };

        private static readonly char[] _wordDelimiters = { ' ', '.', '(', ')', '[', ']', '{', '}', ';', ':', ',', '<', '>', '/', '\\', '"', '\'' };

        private static readonly HashSet<char> _codeSymbols = new()
        {
            '{', '}', '(', ')', '[', ']', ';', ':', ',', '<', '>', '=', '+', '-', '*', '/', '%', '&', '|', '^', '!', '~', '?', '#'
        };

        public ContentProcessor(IFileTypeClassifier fileTypeClassifier)
        {
            _fileTypeClassifier = fileTypeClassifier;
        }

        public ProcessingResult Process(string content)
        {
            if (string.IsNullOrWhiteSpace(content))
            {
                return new ProcessingResult(AppConstants.ClipTypeText, content);
            }

            bool hadLeadingWhitespace = content.Length > content.TrimStart().Length;
            var trimmedContent = content.Trim();

            if (IsColor(trimmedContent))
            {
                return new ProcessingResult(AppConstants.ClipTypeColor, content, hadLeadingWhitespace);
            }

            if (Uri.TryCreate(trimmedContent, UriKind.Absolute, out var uri) && (uri.Scheme == Uri.UriSchemeHttp || uri.Scheme == Uri.UriSchemeHttps))
            {
                return new ProcessingResult(AppConstants.ClipTypeLink, content, hadLeadingWhitespace);
            }

            // Only check for file paths if it's a single-line string to avoid misclassifying code blocks.
            if (!content.Contains('\n', StringComparison.Ordinal))
            {
                var classifiedType = _fileTypeClassifier.Classify(trimmedContent);

                if (classifiedType.StartsWith("file_", StringComparison.Ordinal) || classifiedType == AppConstants.ClipTypeFolder)
                {
                    // New check: If the content looks like a file/folder but doesn't exist, treat it as text.
                    if (!Directory.Exists(trimmedContent) && !File.Exists(trimmedContent))
                    {
                        classifiedType = AppConstants.ClipTypeText;
                    }
                }

                if (classifiedType != AppConstants.ClipTypeText)
                {
                    // If it's a path to a primarily textual file type,
                    // let it fall through to be evaluated as a code snippet or plain text.
                    // Non-textual file types (images, videos, archives) are treated as file clips immediately.
                    bool isTextualFileType = classifiedType is AppConstants.ClipTypeDev or AppConstants.ClipTypeFileText;

                    if (!isTextualFileType)
                    {
                        return new ProcessingResult(classifiedType, content, hadLeadingWhitespace);
                    }
                }
            }

            if (IsCodeSnippet(content))
            {
                return new ProcessingResult(AppConstants.ClipTypeCodeSnippet, content, hadLeadingWhitespace);
            }

            return new ProcessingResult(AppConstants.ClipTypeText, content, hadLeadingWhitespace);
        }

        private static bool IsColor(string input)
        {
            if (input.Length > 100 || input.AsSpan().ContainsAny(_invalidColorChars))
            {
                return false;
            }

            return ColorParser.TryParseColor(input, out _);
        }

        private static bool IsCodeSnippet(string content)
        {
            if (string.IsNullOrWhiteSpace(content) || content.Length < 10) return false;

            var trimmedForCheck = content.Trim();
            if (TextAnalysisUtils.IsJson(trimmedForCheck) || TextAnalysisUtils.IsLikelyXml(trimmedForCheck)) return true;

            var lines = content.Split('\n');
            int lineCount = lines.Length;

            if (lineCount == 1 && content.Length > 250) return false;

            int score = 0;
            int indentedLines = 0;
            int linesWithCodeTerminators = 0;
            int symbolCount = 0;
            int keywordCount = 0;
            int totalChars = 0;

            var linesToScan = lines.Take(50).ToArray();

            foreach (var line in linesToScan)
            {
                var trimmedLine = line.Trim();
                if (string.IsNullOrEmpty(trimmedLine)) continue;

                totalChars += trimmedLine.Length;

                if (line.Length > trimmedLine.Length && (line.StartsWith("    ", StringComparison.Ordinal) || line.StartsWith('\t')))
                {
                    indentedLines++;
                }

                if (trimmedLine.EndsWith('{') || trimmedLine.EndsWith('}') || trimmedLine.EndsWith(';') || trimmedLine.EndsWith(':') || trimmedLine.EndsWith("=>", StringComparison.Ordinal))
                {
                    linesWithCodeTerminators++;
                }

                foreach (char c in trimmedLine)
                {
                    if (_codeSymbols.Contains(c))
                    {
                        symbolCount++;
                    }
                }

                var words = trimmedLine.Split(_wordDelimiters, StringSplitOptions.RemoveEmptyEntries);
                foreach (var word in words)
                {
                    if (_codeKeywords.Contains(word))
                    {
                        keywordCount++;
                    }
                }
            }

            if (totalChars == 0) return false;

            double symbolDensity = (double)symbolCount / totalChars;
            if (symbolDensity > 0.15) score += 4;
            else if (symbolDensity > 0.08) score += 2;

            if (keywordCount > 3) score += 4;
            else if (keywordCount > 0) score += 2;

            if (lineCount > 1)
            {
                double indentedLineRatio = (double)indentedLines / linesToScan.Length;
                if (indentedLineRatio > 0.4) score += 3;

                double terminatorLineRatio = (double)linesWithCodeTerminators / linesToScan.Length;
                if (terminatorLineRatio > 0.4) score += 3;
            }

            return lineCount > 1 ? score >= 5 : score >= 4;
        }
    }
}