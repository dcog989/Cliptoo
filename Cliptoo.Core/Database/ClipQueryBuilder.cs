using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Text;
using Cliptoo.Core.Configuration;
using Microsoft.Data.Sqlite;

namespace Cliptoo.Core.Database
{
    internal static class ClipQueryBuilder
    {
        private static readonly char[] _spaceSeparator = [' '];
        private const string columns = "c.Id, c.Timestamp, c.ClipType, c.SourceApp, c.IsPinned, c.WasTrimmed, c.SizeInBytes, c.PreviewContent";

        public static void BuildGetClipsQuery(SqliteCommand command, uint limit, uint offset, string searchTerm, string filterType)
        {
            var queryBuilder = new StringBuilder();
            var whereConditions = new List<string>();

            var sanitizedTerms = string.IsNullOrWhiteSpace(searchTerm)
                ? new List<string>()
                : searchTerm.Split(_spaceSeparator, StringSplitOptions.RemoveEmptyEntries).Select(term => term.Replace("\"", "\"\"", StringComparison.Ordinal)).Where(sanitized => !string.IsNullOrEmpty(sanitized)).ToList();

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

            queryBuilder.Append($" {orderBy} LIMIT @Limit OFFSET @Offset");
            command.Parameters.AddWithValue("@Limit", limit);
            command.Parameters.AddWithValue("@Offset", offset);
            command.CommandText = queryBuilder.ToString();

            if (LogManager.LoggingLevel == "Debug")
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