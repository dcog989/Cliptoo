using System;
using System.Buffers;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using Cliptoo.Core.Services.Models;

namespace Cliptoo.Core.Services
{
    public class ContentProcessor : IContentProcessor
    {
        private readonly IFileTypeClassifier _fileTypeClassifier;
        private static readonly SearchValues<char> _invalidColorChars = SearchValues.Create("\n\r<>[]");
        private const int MaxLinesForFilePathCheck = 500;

        private static readonly HashSet<string> _codeKeywords = new(StringComparer.Ordinal)
        {
            "public", "private", "protected", "static", "void", "class", "interface", "enum",
            "if", "else", "switch", "case", "for", "foreach", "while", "do", "return", "break",
            "using", "namespace", "import", "from", "new", "var", "let", "const", "get", "set",
            "async", "await", "yield", "try", "catch", "finally", "throw",
            "int", "string", "bool", "double", "float", "char", "object",
            "def", "lambda", "import", "from", "as", "with",
            "function", "return", "new", "var", "let", "const", "=>",
            "SELECT", "FROM", "WHERE", "GROUP", "BY", "ORDER", "JOIN", "INSERT", "UPDATE", "DELETE",
            "<html>", "<div>", "<script>", "<style>", "<body>"
        };

        private static readonly char[] _wordDelimiters = { ' ', '.', '(', ')', '[', ']', '{', '}', ';', ':', ',', '<', '>', '/', '\\', '"', '\'' };

        private static readonly HashSet<char> _codeSymbols = new()
        {
            '{', '}', '(', ')', '[', ']', ';', ':', ',', '<', '>', '=', '+', '-', '*', '/', '%',
            '&', '|', '^', '!', '~', '?', '#'
        };

        public ContentProcessor(IFileTypeClassifier fileTypeClassifier)
        {
            _fileTypeClassifier = fileTypeClassifier;
        }

        public ProcessingResult Process(string content)
        {
            if (string.IsNullOrWhiteSpace(content))
            {
                return new ProcessingResult(AppConstants.ClipTypes.Text, content);
            }

            bool hadLeadingWhitespace = content.Length > content.TrimStart().Length;
            var trimmedContent = content.Trim();

            if (IsColor(trimmedContent))
            {
                return new ProcessingResult(AppConstants.ClipTypes.Color, content, hadLeadingWhitespace);
            }

            if (Uri.IsWellFormedUriString(trimmedContent, UriKind.Absolute))
            {
                return new ProcessingResult(AppConstants.ClipTypes.Link, content, hadLeadingWhitespace);
            }

            if (!content.Contains('\n') && Path.IsPathRooted(trimmedContent) && (Directory.Exists(trimmedContent) || File.Exists(trimmedContent)))
            {
                if (trimmedContent.EndsWith(".url", StringComparison.OrdinalIgnoreCase))
                {
                    var extractedUrl = ParseUrlFile(trimmedContent);
                    if (!string.IsNullOrEmpty(extractedUrl))
                    {
                        return new ProcessingResult(AppConstants.ClipTypes.Link, extractedUrl, hadLeadingWhitespace, Path.GetFileName(trimmedContent));
                    }
                }
                var fileType = _fileTypeClassifier.Classify(trimmedContent);
                return new ProcessingResult(fileType, content, hadLeadingWhitespace);
            }

            if (content.Contains('\n'))
            {
                using (var reader = new StringReader(content))
                {
                    var lines = new List<string>();
                    string? line;
                    int lineCount = 0;
                    bool limitExceeded = false;

                    while ((line = reader.ReadLine()) != null)
                    {
                        lineCount++;
                        if (lineCount > MaxLinesForFilePathCheck)
                        {
                            limitExceeded = true;
                            break;
                        }
                        var trimmedLine = line.Trim();
                        if (!string.IsNullOrEmpty(trimmedLine))
                        {
                            lines.Add(trimmedLine);
                        }
                    }

                    if (!limitExceeded && lines.Count > 0 && lines.All(l => Path.IsPathRooted(l) && (Directory.Exists(l) || File.Exists(l))))
                    {
                        return new ProcessingResult(AppConstants.ClipTypes.Text, content, hadLeadingWhitespace);
                    }
                }
            }

            if (IsCodeSnippet(content))
            {
                return new ProcessingResult(AppConstants.ClipTypes.CodeSnippet, content, hadLeadingWhitespace);
            }

            return new ProcessingResult(AppConstants.ClipTypes.Text, content, hadLeadingWhitespace);
        }

        private string? ParseUrlFile(string filePath)
        {
            try
            {
                var lines = File.ReadAllLines(filePath);
                foreach (var line in lines)
                {
                    if (line.Trim().StartsWith("URL=", StringComparison.OrdinalIgnoreCase))
                    {
                        return line.Substring(line.IndexOf('=') + 1).Trim();
                    }
                }
            }
            catch (Exception ex)
            {
                Configuration.LogManager.Log(ex, $"Failed to parse .url file: {filePath}");
            }
            return null;
        }

        private bool IsColor(string input)
        {
            if (input.Length > 100 || input.AsSpan().ContainsAny(_invalidColorChars))
            {
                return false;
            }

            return ColorParser.TryParseColor(input, out _);
        }

        private bool IsCodeSnippet(string content)
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

                if (line.Length > trimmedLine.Length && (line.StartsWith("    ") || line.StartsWith("\t")))
                {
                    indentedLines++;
                }

                if (trimmedLine.EndsWith('{') || trimmedLine.EndsWith('}') || trimmedLine.EndsWith(';') || trimmedLine.EndsWith(':') || trimmedLine.EndsWith("=>"))
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