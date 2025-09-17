using System;
using System.Collections.Generic;
using System.Diagnostics.CodeAnalysis;
using System.Globalization;
using System.Linq;
using System.Text;
using System.Text.RegularExpressions;
using Cliptoo.Core.Logging;
using Microsoft.Data.Sqlite;

namespace Cliptoo.Core.Database
{
    internal static class ClipQueryBuilder
    {
        private static readonly char[] _spaceSeparator = [' '];
        private const string columns = "c.Id, c.Timestamp, c.ClipType, c.SourceApp, c.IsPinned, c.WasTrimmed, c.SizeInBytes, c.PreviewContent";
        private static readonly Regex FtsSpecialCharsRegex = new("[^a-zA-Z0-9_]");

        [SuppressMessage("Microsoft.Security", "CA2100:Review SQL queries for security vulnerabilities", Justification = "OrderBy clause is constructed from hardcoded, non-user-input strings.")]
        public static void BuildGetClipsQuery(SqliteCommand command, uint limit, uint offset, string searchTerm, string filterType)
        {
            var queryBuilder = new StringBuilder();
            var whereConditions = new List<string>();

            var sanitizedTerms = new List<string>();
            if (!string.IsNullOrWhiteSpace(searchTerm))
            {
                var terms = searchTerm.Split(_spaceSeparator, StringSplitOptions.RemoveEmptyEntries);
                foreach (var term in terms)
                {
                    // Escape double quotes within the term for FTS5
                    var escapedTerm = term.Replace("\"", "\"\"", StringComparison.Ordinal);
                    // If the term contains characters that FTS5 treats as separators or syntax,
                    // it must be enclosed in double quotes to be treated as a single token.
                    if (FtsSpecialCharsRegex.IsMatch(escapedTerm))
                    {
                        sanitizedTerms.Add($"\"{escapedTerm}\"");
                    }
                    else
                    {
                        sanitizedTerms.Add(escapedTerm);
                    }
                }
            }

            if (sanitizedTerms.Count > 0)
            {
                var ftsQuery = string.Join(" ", sanitizedTerms.Select(term => $"{term}*"));
                command.Parameters.AddWithValue("@FtsSearchTerm", ftsQuery);

                queryBuilder.AppendFormat(CultureInfo.InvariantCulture,
                    "SELECT {0}, snippet(clips_fts, 0, '[HL]', '[/HL]', '...', 60) as MatchContext ",
                    columns);

                queryBuilder.Append("FROM clips c JOIN clips_fts ON c.Id = clips_fts.rowid ");

                whereConditions.Add("clips_fts MATCH @FtsSearchTerm");
            }
            else
            {
                queryBuilder.AppendFormat(CultureInfo.InvariantCulture, "SELECT {0} FROM clips c ", columns);
            }

            if (filterType == AppConstants.FilterKeys.Pinned)
            {
                whereConditions.Add("c.IsPinned = 1");
            }
            else if (filterType == AppConstants.ClipTypes.Link)
            {
                whereConditions.Add("(c.ClipType = @FilterTypeLink OR c.ClipType = @FilterTypeFileLink)");
                command.Parameters.AddWithValue("@FilterTypeLink", AppConstants.ClipTypes.Link);
                command.Parameters.AddWithValue("@FilterTypeFileLink", AppConstants.ClipTypes.FileLink);
            }
            else if (filterType != AppConstants.FilterKeys.All)
            {
                whereConditions.Add("c.ClipType = @FilterType");
                command.Parameters.AddWithValue("@FilterType", filterType);
            }

            if (whereConditions.Count > 0)
            {
                queryBuilder.Append("WHERE ").Append(string.Join(" AND ", whereConditions));
            }

            string orderBy = sanitizedTerms.Count > 0
                ? "ORDER BY c.IsPinned DESC, Rank ASC, c.Timestamp DESC"
                : "ORDER BY c.Timestamp DESC";

            queryBuilder.Append(CultureInfo.InvariantCulture, $" {orderBy} LIMIT @Limit OFFSET @Offset");
            command.Parameters.AddWithValue("@Limit", limit);
            command.Parameters.AddWithValue("@Offset", offset);
            command.CommandText = queryBuilder.ToString();

            if (LogManager.LoggingLevel == LogLevel.Debug)
            {
                LogManager.LogDebug($"SQL_QUERY_DIAG: Generated query: {command.CommandText}");
                var paramLog = new StringBuilder("SQL_QUERY_DIAG: Parameters: ");
                foreach (SqliteParameter p in command.Parameters)
                {
                    paramLog.Append(CultureInfo.InvariantCulture, $"{p.ParameterName}='{p.Value}', ");
                }
                LogManager.LogDebug(paramLog.ToString());
            }
        }
    }
}