using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using Microsoft.Data.Sqlite;
using Cliptoo.Core.Configuration; // keep for logmanager
using System.Diagnostics.CodeAnalysis;

namespace Cliptoo.Core.Database
{
    internal static class ClipQueryBuilder
    {
        private static readonly char[] _spaceSeparator = [' '];
        private const string columns = "c.Id, c.Timestamp, c.ClipType, c.SourceApp, c.IsPinned, c.WasTrimmed, c.SizeInBytes, c.PreviewContent";

        [SuppressMessage("Microsoft.Security", "CA2100:Review SQL queries for security vulnerabilities", Justification = "Query is built from safe, hardcoded strings and user input is parameterized.")]
        public static void BuildGetClipsQuery(SqliteCommand command, uint limit, uint offset, string searchTerm, string filterType)
        {
            var queryBuilder = new System.Text.StringBuilder();
            string orderBy;

            var sanitizedTerms = new List<string>();
            if (!string.IsNullOrWhiteSpace(searchTerm))
            {
                sanitizedTerms = searchTerm.Split(_spaceSeparator, StringSplitOptions.RemoveEmptyEntries)
                    .Select(term => term.Replace("\"", "\"\"", StringComparison.Ordinal)) // Escape double quotes for FTS5
                    .Where(sanitized => !string.IsNullOrEmpty(sanitized))
                    .ToList();
            }

            if (sanitizedTerms.Count > 0)
            {
                var ftsQuery = string.Join(" ", sanitizedTerms.Select(term => $"\"{term}\"*"));
                command.Parameters.AddWithValue("@SearchTerm", ftsQuery);

                var likeParams = new List<string>();
                for (int i = 0; i < sanitizedTerms.Count; i++)
                {
                    var paramName = $"@LikeTerm{i}";
                    command.Parameters.AddWithValue(paramName, $"%{sanitizedTerms[i]}%");
                    likeParams.Add($"c.Content LIKE {paramName}");
                }
                var likeCondition = string.Join(" AND ", likeParams);

                var filterConditions = new System.Text.StringBuilder();
                if (filterType == AppConstants.FilterKeys.Pinned)
                {
                    filterConditions.Append("AND c.IsPinned = 1 ");
                }
                else if (filterType == AppConstants.ClipTypes.Link)
                {
                    filterConditions.Append("AND (c.ClipType = @FilterTypeLink OR c.ClipType = @FilterTypeFileLink) ");
                    command.Parameters.AddWithValue("@FilterTypeLink", AppConstants.ClipTypes.Link);
                    command.Parameters.AddWithValue("@FilterTypeFileLink", AppConstants.ClipTypes.FileLink);
                }
                else if (filterType != AppConstants.FilterKeys.All)
                {
                    filterConditions.Append(CultureInfo.InvariantCulture, $"AND c.ClipType = @FilterType ");
                    command.Parameters.AddWithValue("@FilterType", filterType);
                }

                queryBuilder.Append("SELECT * FROM ( ");
                // FTS part
                queryBuilder.Append(CultureInfo.InvariantCulture, $"SELECT {columns}, snippet(clips_fts, 0, '[HL]', '[/HL]', '...', 60) as MatchContext, 0 as SortPriority ");
                queryBuilder.Append("FROM clips c JOIN clips_fts fts ON c.Id = fts.rowid ");
                queryBuilder.Append("WHERE clips_fts MATCH @SearchTerm ");
                queryBuilder.Append(filterConditions);

                queryBuilder.Append("UNION ALL ");

                // LIKE part
                queryBuilder.Append(CultureInfo.InvariantCulture, $"SELECT {columns}, NULL as MatchContext, 1 as SortPriority ");
                queryBuilder.Append("FROM clips c ");
                queryBuilder.Append(CultureInfo.InvariantCulture, $"WHERE {likeCondition} ");
                queryBuilder.Append("AND NOT EXISTS (SELECT 1 FROM clips_fts WHERE clips_fts.rowid = c.Id AND clips_fts MATCH @SearchTerm) ");
                queryBuilder.Append(filterConditions);
                queryBuilder.Append(") ");

                orderBy = "ORDER BY IsPinned DESC, SortPriority ASC, Timestamp DESC";
            }
            else
            {
                queryBuilder.Append(CultureInfo.InvariantCulture, $"SELECT {columns} FROM clips c ");
                orderBy = "ORDER BY c.Timestamp DESC";
                var conditions = new List<string>();
                if (filterType == AppConstants.FilterKeys.Pinned)
                {
                    conditions.Add("c.IsPinned = 1");
                }
                else if (filterType == AppConstants.ClipTypes.Link)
                {
                    conditions.Add("(c.ClipType = @FilterTypeLink OR c.ClipType = @FilterTypeFileLink)");
                    command.Parameters.AddWithValue("@FilterTypeLink", AppConstants.ClipTypes.Link);
                    command.Parameters.AddWithValue("@FilterTypeFileLink", AppConstants.ClipTypes.FileLink);
                }
                else if (filterType != AppConstants.FilterKeys.All)
                {
                    conditions.Add("c.ClipType = @FilterType");
                    command.Parameters.AddWithValue("@FilterType", filterType);
                }

                if (conditions.Count > 0)
                {
                    queryBuilder.Append("WHERE ").Append(string.Join(" AND ", conditions));
                }
            }

            queryBuilder.Append(CultureInfo.InvariantCulture, $" {orderBy} LIMIT @Limit OFFSET @Offset");
            command.Parameters.AddWithValue("@Limit", limit);
            command.Parameters.AddWithValue("@Offset", offset);
            command.CommandText = queryBuilder.ToString();

            if (LogManager.LoggingLevel == "Debug")
            {
                LogManager.LogDebug($"SQL_QUERY_DIAG: Generated query: {command.CommandText}");
                var paramLog = new System.Text.StringBuilder("SQL_QUERY_DIAG: Parameters: ");
                foreach (SqliteParameter p in command.Parameters)
                {
                    paramLog.Append(CultureInfo.InvariantCulture, $"{p.ParameterName}='{p.Value}', ");
                }
                LogManager.LogDebug(paramLog.ToString());
            }
        }

    }
}