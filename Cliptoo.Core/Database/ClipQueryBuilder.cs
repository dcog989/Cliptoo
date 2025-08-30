using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using Microsoft.Data.Sqlite;

namespace Cliptoo.Core.Database
{
    internal static class ClipQueryBuilder
    {
        private static readonly char[] _spaceSeparator = [' '];

        public static void BuildGetClipsQuery(SqliteCommand command, uint limit, uint offset, string searchTerm, string filterType)
        {
            const string columns = "c.Id, c.Timestamp, c.ClipType, c.SourceApp, c.IsPinned, c.WasTrimmed, c.SizeInBytes, c.PreviewContent";

            var queryBuilder = new System.Text.StringBuilder();
            var conditions = new List<string>();
            string orderBy;

            if (!string.IsNullOrWhiteSpace(searchTerm))
            {
                queryBuilder.Append($"SELECT {columns}, snippet(clips_fts, 0, '[HL]', '[/HL]', '...', 60) as MatchContext FROM clips c JOIN clips_fts fts ON c.Id = fts.rowid ");
                conditions.Add("clips_fts MATCH @SearchTerm");

                var terms = searchTerm.Split(_spaceSeparator, StringSplitOptions.RemoveEmptyEntries)
                                      .Select(term => term.Replace("\"", "\"\""))
                                      .ToList();

                string ftsQuery;
                if (terms.Count > 1)
                {
                    // A query that looks for the terms near each other. Terms must not be quoted inside NEAR.
                    var nearTerms = string.Join(" ", terms.Select(t => $"{t}*"));
                    var nearQuery = $"NEAR({nearTerms})";

                    // A fallback query that finds all terms as prefixes anywhere in the text.
                    var allTermsQuery = string.Join(" AND ", terms.Select(t => $"{t}*"));

                    // FTS5 ranks the leftmost part of an OR query higher.
                    ftsQuery = $"({nearQuery}) OR ({allTermsQuery})";
                }
                else if (terms.Count == 1)
                {
                    ftsQuery = $"{terms[0]}*";
                }
                else
                {
                    ftsQuery = string.Empty;
                }

                command.Parameters.AddWithValue("@SearchTerm", ftsQuery);

                orderBy = "ORDER BY rank, c.IsPinned DESC, c.Timestamp DESC";
            }
            else
            {
                queryBuilder.Append($"SELECT {columns} FROM clips c ");
                orderBy = "ORDER BY c.IsPinned DESC, c.Timestamp DESC";
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
        }

    }
}