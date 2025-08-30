using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using Microsoft.Data.Sqlite;
using Cliptoo.Core.Configuration;

namespace Cliptoo.Core.Database
{
    internal static class ClipQueryBuilder
    {
        private static readonly char[] _spaceSeparator = [' '];



        public static void BuildGetClipsQuery(SqliteCommand command, uint limit, uint offset, string searchTerm, string filterType)
        {
            const string columns = "c.Id, c.Timestamp, c.ClipType, c.SourceApp, c.IsPinned, c.WasTrimmed, c.SizeInBytes, c.PreviewContent";
            LogManager.LogDebug($"SEARCH_DIAG_BUILDER: Building query with searchTerm='{searchTerm}', filterType='{filterType}'");

            var queryBuilder = new System.Text.StringBuilder();
            var conditions = new List<string>();
            string orderBy;

            if (!string.IsNullOrWhiteSpace(searchTerm))
            {
                LogManager.LogDebug("SEARCH_DIAG_BUILDER: SearchTerm is not null or whitespace. Entering search query block.");
                queryBuilder.Append($"SELECT {columns}, snippet(clips_fts, 0, '[HL]', '[/HL]', '...', 60) as MatchContext FROM clips c JOIN clips_fts fts ON c.Id = fts.rowid ");
                conditions.Add("clips_fts MATCH @SearchTerm");

                var ftsQuery = string.Join(" AND ", searchTerm.Split(_spaceSeparator, StringSplitOptions.RemoveEmptyEntries).Select(term => $"{term.Replace("\"", "\"\"")}*"));

                LogManager.LogDebug($"SEARCH_DIAG_BUILDER: Generated FTS query string: '{ftsQuery}'");
                command.Parameters.AddWithValue("@SearchTerm", ftsQuery);

                orderBy = "ORDER BY c.IsPinned DESC, rank, c.Timestamp DESC";
            }
            else
            {
                LogManager.LogDebug("SEARCH_DIAG_BUILDER: SearchTerm is null or whitespace. Entering non-search query block.");
                queryBuilder.Append($"SELECT {columns} FROM clips c ");
                orderBy = "ORDER BY c.Timestamp DESC";
            }

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

            queryBuilder.Append(CultureInfo.InvariantCulture, $" {orderBy} LIMIT @Limit OFFSET @Offset");
            command.Parameters.AddWithValue("@Limit", limit);
            command.Parameters.AddWithValue("@Offset", offset);
            command.CommandText = queryBuilder.ToString();
            LogManager.LogDebug($"SEARCH_DIAG_BUILDER: Final command text: {command.CommandText}");
        }


    }
}