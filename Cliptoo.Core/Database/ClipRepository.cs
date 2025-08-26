using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Cliptoo.Core.Database.Models;
using Microsoft.Data.Sqlite;

namespace Cliptoo.Core.Database
{
    public class ClipRepository : RepositoryBase, IClipRepository
    {
        private const int MaxPreviewBytes = 5 * 1024;

        public ClipRepository(string dbPath) : base(dbPath) { }

        private static string CreatePreview(string content)
        {
            if (Encoding.UTF8.GetByteCount(content) <= MaxPreviewBytes)
            {
                return content;
            }

            var encoder = Encoding.UTF8.GetEncoder();
            var bytes = new byte[MaxPreviewBytes];
            encoder.Convert(content.AsSpan(), bytes, true, out int charsUsed, out _, out _);
            return content.Substring(0, charsUsed);
        }

        public async Task<List<Clip>> GetClipsAsync(uint limit, uint offset, string searchTerm, string filterType, CancellationToken cancellationToken)
        {
            var clips = new List<Clip>();
            await using var connection = await GetOpenConnectionAsync().ConfigureAwait(false);
            await using var command = connection.CreateCommand();

            const string columns = "c.Id, c.Timestamp, c.ClipType, c.SourceApp, c.IsPinned, c.WasTrimmed, c.SizeInBytes, c.PreviewContent";

            var queryBuilder = new System.Text.StringBuilder();
            var conditions = new List<string>();
            string orderBy;

            if (!string.IsNullOrWhiteSpace(searchTerm))
            {
                queryBuilder.Append($"SELECT {columns}, snippet(clips_fts, 0, '[HL]', '[/HL]', '...', 15) as MatchContext FROM clips c JOIN clips_fts fts ON c.Id = fts.rowid ");
                conditions.Add("clips_fts MATCH @SearchTerm");

                var ftsQuery = string.Join(" ", searchTerm.Split(new[] { ' ' }, StringSplitOptions.RemoveEmptyEntries)
                                                            .Select(term => $"\"{term.Replace("\"", "\"\"")}\"*"));
                command.Parameters.AddWithValue("@SearchTerm", ftsQuery);

                orderBy = "ORDER BY c.IsPinned DESC, rank, c.Timestamp DESC";
            }
            else
            {
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

            queryBuilder.Append($" {orderBy} LIMIT @Limit OFFSET @Offset");
            command.Parameters.AddWithValue("@Limit", limit);
            command.Parameters.AddWithValue("@Offset", offset);
            command.CommandText = queryBuilder.ToString();

            var ftsQueryForLog = command.Parameters.Contains("@SearchTerm") ? command.Parameters["@SearchTerm"].Value : "N/A";
            Configuration.LogManager.LogDebug($"SEARCH_DIAG: Executing query. FTS Query: '{ftsQueryForLog}', Filter: {filterType}");
            Configuration.LogManager.LogDebug($"SEARCH_DIAG: SQL: {command.CommandText}");
            var stopwatch = System.Diagnostics.Stopwatch.StartNew();

            await using var reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false);

            stopwatch.Stop();
            Configuration.LogManager.LogDebug($"SEARCH_DIAG: Query executed in {stopwatch.ElapsedMilliseconds}ms.");

            var ordinals = new
            {
                Id = reader.GetOrdinal("Id"),
                Timestamp = reader.GetOrdinal("Timestamp"),
                ClipType = reader.GetOrdinal("ClipType"),
                SourceApp = reader.GetOrdinal("SourceApp"),
                IsPinned = reader.GetOrdinal("IsPinned"),
                WasTrimmed = reader.GetOrdinal("WasTrimmed"),
                SizeInBytes = reader.GetOrdinal("SizeInBytes"),
                PreviewContent = reader.GetOrdinal("PreviewContent"),
                MatchContext = HasColumn(reader, "MatchContext") ? reader.GetOrdinal("MatchContext") : -1
            };

            while (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
            {
                var sourceApp = await reader.IsDBNullAsync(ordinals.SourceApp, CancellationToken.None).ConfigureAwait(false) ? null : reader.GetString(ordinals.SourceApp);
                var previewContent = await reader.IsDBNullAsync(ordinals.PreviewContent, CancellationToken.None).ConfigureAwait(false) ? null : reader.GetString(ordinals.PreviewContent);
                var wasTrimmedDbNull = await reader.IsDBNullAsync(ordinals.WasTrimmed, CancellationToken.None).ConfigureAwait(false);
                var matchContext = ordinals.MatchContext != -1 && !await reader.IsDBNullAsync(ordinals.MatchContext, CancellationToken.None).ConfigureAwait(false) ? reader.GetString(ordinals.MatchContext) : null;

                clips.Add(new Clip
                {
                    Id = reader.GetInt32(ordinals.Id),
                    Timestamp = DateTime.Parse(reader.GetString(ordinals.Timestamp)).ToLocalTime(),
                    ClipType = reader.GetString(ordinals.ClipType),
                    SourceApp = sourceApp,
                    IsPinned = reader.GetInt64(ordinals.IsPinned) == 1,
                    WasTrimmed = !wasTrimmedDbNull && reader.GetInt64(ordinals.WasTrimmed) == 1,
                    SizeInBytes = reader.GetInt64(ordinals.SizeInBytes),
                    PreviewContent = previewContent,
                    MatchContext = matchContext
                });
            }
            return clips;
        }

        public async Task<Clip> GetClipPreviewContentByIdAsync(int id)
        {
            await using var connection = await GetOpenConnectionAsync().ConfigureAwait(false);
            await using var command = connection.CreateCommand();
            command.CommandText = "SELECT Id, PreviewContent, Timestamp, ClipType, SourceApp, IsPinned, WasTrimmed, SizeInBytes FROM clips WHERE Id = @Id";
            command.Parameters.AddWithValue("@Id", id);

            await using var reader = await command.ExecuteReaderAsync().ConfigureAwait(false);
            if (await reader.ReadAsync().ConfigureAwait(false))
            {
                var idOrdinal = reader.GetOrdinal("Id");
                var previewContentOrdinal = reader.GetOrdinal("PreviewContent");
                var timestampOrdinal = reader.GetOrdinal("Timestamp");
                var clipTypeOrdinal = reader.GetOrdinal("ClipType");
                var sourceAppOrdinal = reader.GetOrdinal("SourceApp");
                var isPinnedOrdinal = reader.GetOrdinal("IsPinned");
                var wasTrimmedOrdinal = reader.GetOrdinal("WasTrimmed");
                var sizeInBytesOrdinal = reader.GetOrdinal("SizeInBytes");

                var previewContent = await reader.IsDBNullAsync(previewContentOrdinal, CancellationToken.None).ConfigureAwait(false) ? null : reader.GetString(previewContentOrdinal);
                var sourceApp = await reader.IsDBNullAsync(sourceAppOrdinal, CancellationToken.None).ConfigureAwait(false) ? null : reader.GetString(sourceAppOrdinal);

                return new Clip
                {
                    Id = reader.GetInt32(idOrdinal),
                    PreviewContent = previewContent,
                    Timestamp = DateTime.Parse(reader.GetString(timestampOrdinal)).ToLocalTime(),
                    ClipType = reader.GetString(clipTypeOrdinal),
                    SourceApp = sourceApp,
                    IsPinned = reader.GetInt64(isPinnedOrdinal) == 1,
                    WasTrimmed = reader.GetInt64(wasTrimmedOrdinal) == 1,
                    SizeInBytes = reader.GetInt64(sizeInBytesOrdinal)
                };
            }
            throw new InvalidOperationException($"Clip with ID {id} not found.");
        }

        public async Task<int> AddClipAsync(string content, string clipType, string? sourceApp, bool wasTrimmed)
        {
            await using var connection = await GetOpenConnectionAsync().ConfigureAwait(false);
            var contentSize = (long)System.Text.Encoding.UTF8.GetByteCount(content);
            var hash = Convert.ToHexString(System.Security.Cryptography.SHA256.HashData(System.Text.Encoding.UTF8.GetBytes(content)));

            await using (var selectCmd = connection.CreateCommand())
            {
                selectCmd.CommandText = "SELECT Id FROM clips WHERE ContentHash = @Hash";
                selectCmd.Parameters.AddWithValue("@Hash", hash);
                var existingIdObj = await selectCmd.ExecuteScalarAsync().ConfigureAwait(false);
                if (existingIdObj != null)
                {
                    var existingId = Convert.ToInt32(existingIdObj, System.Globalization.CultureInfo.InvariantCulture);
                    await using var updateCmd = connection.CreateCommand();
                    updateCmd.CommandText = "UPDATE clips SET Timestamp = @Timestamp, SourceApp = @SourceApp WHERE Id = @Id";
                    updateCmd.Parameters.AddWithValue("@Timestamp", DateTime.UtcNow.ToString("o"));
                    updateCmd.Parameters.AddWithValue("@SourceApp", sourceApp ?? (object)DBNull.Value);
                    updateCmd.Parameters.AddWithValue("@Id", existingId);
                    await updateCmd.ExecuteNonQueryAsync().ConfigureAwait(false);
                    return existingId;
                }
            }

            var previewContent = CreatePreview(content);

            await using var insertCmd = connection.CreateCommand();
            insertCmd.CommandText = @"
                INSERT INTO clips (Content, ContentHash, PreviewContent, ClipType, SourceApp, Timestamp, IsPinned, WasTrimmed, SizeInBytes)
                VALUES (@Content, @ContentHash, @PreviewContent, @ClipType, @SourceApp, @Timestamp, 0, @WasTrimmed, @SizeInBytes);
                SELECT last_insert_rowid();";
            insertCmd.Parameters.AddWithValue("@Content", content);
            insertCmd.Parameters.AddWithValue("@ContentHash", hash);
            insertCmd.Parameters.AddWithValue("@PreviewContent", previewContent);
            insertCmd.Parameters.AddWithValue("@ClipType", clipType);
            insertCmd.Parameters.AddWithValue("@SourceApp", sourceApp ?? (object)DBNull.Value);
            insertCmd.Parameters.AddWithValue("@Timestamp", DateTime.UtcNow.ToString("o"));
            insertCmd.Parameters.AddWithValue("@WasTrimmed", wasTrimmed ? 1 : 0);
            insertCmd.Parameters.AddWithValue("@SizeInBytes", contentSize);
            var newId = (long)(await insertCmd.ExecuteScalarAsync().ConfigureAwait(false) ?? -1L);

            await using (var updateTotalCmd = connection.CreateCommand())
            {
                updateTotalCmd.CommandText = "UPDATE stats SET Value = COALESCE(Value, 0) + 1 WHERE Key = 'TotalClipsEver'";
                await updateTotalCmd.ExecuteNonQueryAsync().ConfigureAwait(false);
            }

            return (int)newId;
        }

        public async Task UpdateClipContentAsync(int id, string content)
        {
            await using var connection = await GetOpenConnectionAsync().ConfigureAwait(false);
            await using var command = connection.CreateCommand();
            var previewContent = CreatePreview(content);
            var hash = Convert.ToHexString(System.Security.Cryptography.SHA256.HashData(System.Text.Encoding.UTF8.GetBytes(content)));

            command.CommandText = "UPDATE clips SET Content = @Content, ContentHash = @ContentHash, PreviewContent = @PreviewContent, SizeInBytes = @SizeInBytes WHERE Id = @Id";
            command.Parameters.AddWithValue("@Content", content);
            command.Parameters.AddWithValue("@ContentHash", hash);
            command.Parameters.AddWithValue("@PreviewContent", previewContent);
            command.Parameters.AddWithValue("@SizeInBytes", (long)System.Text.Encoding.UTF8.GetByteCount(content));
            command.Parameters.AddWithValue("@Id", id);
            await command.ExecuteNonQueryAsync().ConfigureAwait(false);
        }

        public async IAsyncEnumerable<string> GetAllImageClipPathsAsync()
        {
            await using var connection = await GetOpenConnectionAsync().ConfigureAwait(false);
            await using var command = connection.CreateCommand();
            command.CommandText = "SELECT Content FROM clips WHERE ClipType = @ClipType";
            command.Parameters.AddWithValue("@ClipType", AppConstants.ClipTypes.Image);

            await using var reader = await command.ExecuteReaderAsync().ConfigureAwait(false);
            while (await reader.ReadAsync().ConfigureAwait(false))
            {
                yield return reader.GetString(0);
            }
        }

        public async IAsyncEnumerable<string> GetAllLinkClipUrlsAsync()
        {
            await using var connection = await GetOpenConnectionAsync().ConfigureAwait(false);
            await using var command = connection.CreateCommand();
            command.CommandText = "SELECT DISTINCT Content FROM clips WHERE ClipType = @ClipType";
            command.Parameters.AddWithValue("@ClipType", AppConstants.ClipTypes.Link);

            await using var reader = await command.ExecuteReaderAsync().ConfigureAwait(false);
            while (await reader.ReadAsync().ConfigureAwait(false))
            {
                yield return reader.GetString(0);
            }
        }

        public async Task DeleteClipAsync(int id)
        {
            await using var connection = await GetOpenConnectionAsync().ConfigureAwait(false);
            await using var command = connection.CreateCommand();
            command.CommandText = "DELETE FROM clips WHERE Id = @Id";
            command.Parameters.AddWithValue("@Id", id);
            await command.ExecuteNonQueryAsync().ConfigureAwait(false);
        }

        public async Task TogglePinAsync(int id, bool isPinned)
        {
            await using var connection = await GetOpenConnectionAsync().ConfigureAwait(false);
            await using var command = connection.CreateCommand();
            command.CommandText = "UPDATE clips SET IsPinned = @IsPinned WHERE Id = @Id";
            command.Parameters.AddWithValue("@IsPinned", isPinned ? 1 : 0);
            command.Parameters.AddWithValue("@Id", id);
            await command.ExecuteNonQueryAsync().ConfigureAwait(false);
        }

        public async Task UpdateTimestampAsync(int id)
        {
            await using var connection = await GetOpenConnectionAsync().ConfigureAwait(false);
            await using var command = connection.CreateCommand();
            command.CommandText = "UPDATE clips SET Timestamp = @Timestamp WHERE Id = @Id";
            command.Parameters.AddWithValue("@Timestamp", DateTime.UtcNow.ToString("o"));
            command.Parameters.AddWithValue("@Id", id);
            await command.ExecuteNonQueryAsync().ConfigureAwait(false);
        }

        public async Task<Clip> GetClipByIdAsync(int id)
        {
            await using var connection = await GetOpenConnectionAsync().ConfigureAwait(false);
            await using var command = connection.CreateCommand();
            command.CommandText = "SELECT * FROM clips WHERE Id = @Id";
            command.Parameters.AddWithValue("@Id", id);

            await using var reader = await command.ExecuteReaderAsync().ConfigureAwait(false);
            if (await reader.ReadAsync().ConfigureAwait(false))
            {
                var idOrdinal = reader.GetOrdinal("Id");
                var contentOrdinal = reader.GetOrdinal("Content");
                var previewContentOrdinal = reader.GetOrdinal("PreviewContent");
                var timestampOrdinal = reader.GetOrdinal("Timestamp");
                var clipTypeOrdinal = reader.GetOrdinal("ClipType");
                var sourceAppOrdinal = reader.GetOrdinal("SourceApp");
                var isPinnedOrdinal = reader.GetOrdinal("IsPinned");
                var wasTrimmedOrdinal = reader.GetOrdinal("WasTrimmed");
                var sizeInBytesOrdinal = reader.GetOrdinal("SizeInBytes");

                return new Clip
                {
                    Id = reader.GetInt32(idOrdinal),
                    Content = await reader.IsDBNullAsync(contentOrdinal, CancellationToken.None).ConfigureAwait(false) ? null : reader.GetString(contentOrdinal),
                    PreviewContent = await reader.IsDBNullAsync(previewContentOrdinal, CancellationToken.None).ConfigureAwait(false) ? null : reader.GetString(previewContentOrdinal),
                    Timestamp = DateTime.Parse(reader.GetString(timestampOrdinal)).ToLocalTime(),
                    ClipType = reader.GetString(clipTypeOrdinal),
                    SourceApp = await reader.IsDBNullAsync(sourceAppOrdinal, CancellationToken.None).ConfigureAwait(false) ? null : reader.GetString(sourceAppOrdinal),
                    IsPinned = reader.GetInt64(isPinnedOrdinal) == 1,
                    WasTrimmed = reader.GetInt64(wasTrimmedOrdinal) == 1,
                    SizeInBytes = reader.GetInt64(sizeInBytesOrdinal)
                };
            }
            throw new InvalidOperationException($"Clip with ID {id} not found.");
        }

        private static bool HasColumn(SqliteDataReader reader, string columnName)
        {
            for (int i = 0; i < reader.FieldCount; i++)
            {
                if (reader.GetName(i).Equals(columnName, StringComparison.OrdinalIgnoreCase))
                    return true;
            }
            return false;
        }

        public async Task<List<Clip>> GetAllFileBasedClipsAsync()
        {
            var clips = new List<Clip>();
            await using var connection = await GetOpenConnectionAsync().ConfigureAwait(false);
            await using var command = connection.CreateCommand();
            command.CommandText = "SELECT Id, Content, ClipType FROM clips WHERE ClipType LIKE 'file_%' OR ClipType = 'folder'";

            await using var reader = await command.ExecuteReaderAsync().ConfigureAwait(false);
            while (await reader.ReadAsync().ConfigureAwait(false))
            {
                clips.Add(new Clip
                {
                    Id = reader.GetInt32(0),
                    Content = reader.GetString(1),
                    ClipType = reader.GetString(2)
                });
            }
            return clips;
        }

        public async Task UpdateClipTypesAsync(Dictionary<int, string> updates)
        {
            await using var connection = await GetOpenConnectionAsync().ConfigureAwait(false);
            await using var transaction = (SqliteTransaction)await connection.BeginTransactionAsync().ConfigureAwait(false);

            foreach (var update in updates)
            {
                await using var command = connection.CreateCommand();
                command.Transaction = transaction;
                command.CommandText = "UPDATE clips SET ClipType = @ClipType WHERE Id = @Id";
                command.Parameters.AddWithValue("@Id", update.Key);
                command.Parameters.AddWithValue("@ClipType", update.Value);
                await command.ExecuteNonQueryAsync().ConfigureAwait(false);
            }

            await transaction.CommitAsync().ConfigureAwait(false);
        }
    }
}