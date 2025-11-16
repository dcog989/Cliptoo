using System;
using System.Collections.Generic;
using System.Diagnostics.CodeAnalysis;
using System.Linq;
using System.Text;
using System.Text.RegularExpressions;
using Cliptoo.Core.Logging;
using Microsoft.Data.Sqlite;

namespace Cliptoo.Core.Database
{
    internal static partial class ClipQueryBuilder
    {
        private static readonly char[] _spaceSeparator = [' '];
        private const string Columns = "c.Id, c.Timestamp, c.ClipType, c.SourceApp, c.IsFavorite, c.WasTrimmed, c.SizeInBytes, c.PreviewContent, c.PasteCount, c.Tags";

        // FTS5 special characters that need quoting: double quotes, and characters that could be interpreted as operators
        // This pattern matches anything that's not alphanumeric, underscore, or basic punctuation that doesn't interfere with FTS5
        [GeneratedRegex(@"[^\w\s-]", RegexOptions.Compiled)]
        private static partial Regex FtsSpecialCharsRegex();

        [SuppressMessage("Microsoft.Security", "CA2100:Review SQL queries for security vulnerabilities", Justification = "OrderBy clause is constructed from hardcoded, non-user-input strings.")]
        public static void BuildGetClipsQuery(
            SqliteCommand command,
            uint limit,
            uint offset,
            string searchTerm,
            string filterType,
            string tagSearchPrefix)
        {
            ArgumentNullException.ThrowIfNull(command);

            try
            {
                var queryBuilder = new StringBuilder();
                var whereConditions = new List<string>();
                var isTagSearch = !string.IsNullOrEmpty(tagSearchPrefix)
                    && !string.IsNullOrEmpty(searchTerm)
                    && searchTerm.StartsWith(tagSearchPrefix, StringComparison.Ordinal)
                    && searchTerm.Length > tagSearchPrefix.Length; // Ensure there's content after the prefix

                var actualSearchTerm = isTagSearch
                    ? searchTerm.Substring(tagSearchPrefix.Length)
                    : searchTerm;

                var sanitizedTerms = new List<string>();
                if (!string.IsNullOrWhiteSpace(actualSearchTerm))
                {
                    var terms = actualSearchTerm.Split(_spaceSeparator, StringSplitOptions.RemoveEmptyEntries);
                    foreach (var term in terms)
                    {
                        var sanitizedTerm = SanitizeFtsSearchTerm(term);
                        if (!string.IsNullOrEmpty(sanitizedTerm))
                        {
                            sanitizedTerms.Add(sanitizedTerm);
                        }
                    }
                }

                if (sanitizedTerms.Count > 0)
                {
                    BuildSearchQuery(queryBuilder, command, sanitizedTerms, isTagSearch);
                    whereConditions.Add("clips_fts MATCH @FtsSearchTerm");
                }
                else
                {
                    BuildDefaultQuery(queryBuilder);
                }

                AddFilterConditions(whereConditions, command, filterType);

                if (whereConditions.Count > 0)
                {
                    queryBuilder.Append("WHERE ").Append(string.Join(" AND ", whereConditions));
                }

                string orderBy = sanitizedTerms.Count > 0
                    ? " ORDER BY c.IsFavorite DESC, (rank - Hotness * 5.0) ASC, c.Timestamp DESC"
                    : " ORDER BY c.Timestamp DESC";

                queryBuilder.Append(orderBy);
                queryBuilder.Append(" LIMIT @Limit OFFSET @Offset");

                command.Parameters.AddWithValue("@Limit", limit);
                command.Parameters.AddWithValue("@Offset", offset);
                command.CommandText = queryBuilder.ToString();

                LogQueryIfDebug(command);
            }
            catch (Exception ex)
            {
                LogManager.LogError($"Failed to build query for search term '{searchTerm}': {ex.Message}");
                throw new InvalidOperationException($"Failed to build search query. Search term may contain invalid characters.", ex);
            }
        }

        private static string SanitizeFtsSearchTerm(string term)
        {
            if (string.IsNullOrWhiteSpace(term))
                return string.Empty;

            // Remove or escape potentially dangerous SQL characters
            // Double-dash (--) is a SQL comment, semicolon can terminate statements
            var safeTerm = term
                .Replace("--", "", StringComparison.Ordinal)  // Remove SQL comments
                .Replace(";", "", StringComparison.Ordinal);  // Remove statement terminators

            if (string.IsNullOrWhiteSpace(safeTerm))
                return string.Empty;

            // Escape double quotes within the term for FTS5
            var escapedTerm = safeTerm.Replace("\"", "\"\"", StringComparison.Ordinal);

            // If the term contains FTS5 special characters or operators,
            // it must be enclosed in double quotes to be treated as a single token.
            // This prevents terms like "C++" or "email@domain.com" from being misinterpreted.
            // When quoted, we should NOT append the prefix operator (*) as it creates invalid FTS5 syntax.
            if (FtsSpecialCharsRegex().IsMatch(escapedTerm) || IsFtsReservedWord(escapedTerm))
            {
                return $"\"{escapedTerm}\"";
            }

            return escapedTerm;
        }

        private static bool IsFtsReservedWord(string term)
        {
            // Check for FTS5 reserved words/operators that need quoting
            var upperTerm = term.ToUpperInvariant();
            return upperTerm is "AND" or "OR" or "NOT" or "NEAR";
        }

        private static void BuildSearchQuery(
            StringBuilder queryBuilder,
            SqliteCommand command,
            List<string> sanitizedTerms,
            bool isTagSearch)
        {
            // Add prefix operator (*) only to non-quoted terms
            var ftsQuery = string.Join(" ", sanitizedTerms.Select(term =>
                term.StartsWith('"') && term.EndsWith('"')
                    ? term  // Already quoted - don't add *
                    : $"{term}*"  // Not quoted - add prefix operator
            ));

            if (isTagSearch)
            {
                ftsQuery = $"Tags : ({ftsQuery})";
            }

            command.Parameters.AddWithValue("@FtsSearchTerm", ftsQuery);

            // The snippet should always be generated from the main content (column 0),
            // even during a tag search. The MATCH clause still correctly filters by tag.
            const char snippetColumn = '0';

            queryBuilder.Append("SELECT ")
                        .Append(Columns)
                        .Append(", snippet(clips_fts, ")
                        .Append(snippetColumn)
                        .Append(", '[HL]', '[/HL]', '...',60) as MatchContext,")
                        .Append(" (c.PasteCount + 1.0) / (MAX(0.0, (julianday('now') - julianday(c.Timestamp)) * 24.0) + 2.0) AS Hotness ")
                        .Append("FROM clips c JOIN clips_fts ON c.Id = clips_fts.rowid ");
        }

        private static void BuildDefaultQuery(StringBuilder queryBuilder)
        {
            // For the default view (no search term), keep the result schema consistent.
            queryBuilder.Append("SELECT ")
                        .Append(Columns)
                        .Append(", NULL AS MatchContext,0 AS Hotness ")
                        .Append("FROM clips c ");
        }

        private static void AddFilterConditions(
            List<string> whereConditions,
            SqliteCommand command,
            string filterType)
        {
            if (filterType == AppConstants.FilterKeyFavorite)
            {
                whereConditions.Add("c.IsFavorite = 1");
            }
            else if (filterType == AppConstants.ClipTypeLink)
            {
                whereConditions.Add("(c.ClipType = @FilterTypeLink OR c.ClipType = @FilterTypeFileLink)");
                command.Parameters.AddWithValue("@FilterTypeLink", AppConstants.ClipTypeLink);
                command.Parameters.AddWithValue("@FilterTypeFileLink", AppConstants.ClipTypeFileLink);
            }
            else if (filterType != AppConstants.FilterKeyAll)
            {
                whereConditions.Add("c.ClipType = @FilterType");
                command.Parameters.AddWithValue("@FilterType", filterType);
            }
        }

        private static void LogQueryIfDebug(SqliteCommand command)
        {
            if (LogManager.LoggingLevel != LogLevel.Debug)
                return;

            LogManager.LogDebug($"SQL_QUERY_DIAG: Generated query: {command.CommandText}");

            var paramLog = new StringBuilder("SQL_QUERY_DIAG: Parameters: ");
            foreach (SqliteParameter p in command.Parameters)
            {
                paramLog.Append(p.ParameterName)
                        .Append('=')
                        .Append('\'')
                        .Append(p.Value)
                        .Append('\'')
                        .Append(',')
                        .Append(' ');
            }

            LogManager.LogDebug(paramLog.ToString());
        }
    }
}
