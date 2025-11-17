using System;
using System.Collections.Generic;
using System.Globalization;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Cliptoo.Core.Database.Models;
using Cliptoo.Core.Services;
using Microsoft.Data.Sqlite;

namespace Cliptoo.Core.Database
{
    public class ClipRepository : RepositoryBase, IClipRepository
    {
        private const int MaxPreviewBytes = 5 * 1024;

        public ClipRepository(string dbPath) : base(dbPath) { }

        private static string CreatePreview(string content)
        {
            return ServiceUtils.TruncateToUtf8ByteLimit(content, MaxPreviewBytes);
        }

        private static Clip MapPreviewClipFromReader(SqliteDataReader reader)
        {
            var ordinals = new
            {
                Id = reader.GetOrdinal("Id"),
                Timestamp = reader.GetOrdinal("Timestamp"),
                ClipType = reader.GetOrdinal("ClipType"),
                SourceApp = reader.GetOrdinal("SourceApp"),
                IsFavorite = reader.GetOrdinal("IsFavorite"),
                WasTrimmed = reader.GetOrdinal("WasTrimmed"),
                SizeInBytes = reader.GetOrdinal("SizeInBytes"),
                PreviewContent = reader.GetOrdinal("PreviewContent"),
                PasteCount = reader.GetOrdinal("PasteCount"),
                Tags = reader.GetOrdinal("Tags"),
                MatchContext = HasColumn(reader, "MatchContext") ? reader.GetOrdinal("MatchContext") : -1
            };

            return new Clip
            {
                Id = reader.GetInt32(ordinals.Id),
                Timestamp = DateTime.Parse(reader.GetString(ordinals.Timestamp), CultureInfo.InvariantCulture, DateTimeStyles.RoundtripKind).ToLocalTime(),
                ClipType = reader.GetString(ordinals.ClipType),
                SourceApp = reader.IsDBNull(ordinals.SourceApp) ? null : reader.GetString(ordinals.SourceApp),
                IsFavorite = reader.GetInt64(ordinals.IsFavorite) == 1,
                WasTrimmed = reader.GetInt64(ordinals.WasTrimmed) == 1,
                SizeInBytes = reader.GetInt64(ordinals.SizeInBytes),
                PreviewContent = reader.IsDBNull(ordinals.PreviewContent) ? null : reader.GetString(ordinals.PreviewContent),
                PasteCount = reader.GetInt32(ordinals.PasteCount),
                Tags = reader.IsDBNull(ordinals.Tags) ? null : reader.GetString(ordinals.Tags),
                MatchContext = ordinals.MatchContext != -1 && !reader.IsDBNull(ordinals.MatchContext) ? reader.GetString(ordinals.MatchContext) : null
            };
        }

        public async Task<List<Clip>> GetClipsAsync(uint limit, uint offset, string searchTerm, string filterType, string tagSearchPrefix, CancellationToken cancellationToken)
        {
            ArgumentNullException.ThrowIfNull(searchTerm);
            ArgumentNullException.ThrowIfNull(filterType);
            ArgumentNullException.ThrowIfNull(tagSearchPrefix);

            var clips = new List<Clip>();
            SqliteConnection? connection = null;
            SqliteDataReader? reader = null;
            try
            {
                connection = await GetOpenConnectionAsync().ConfigureAwait(false);
                using var command = connection.CreateCommand();

                ClipQueryBuilder.BuildGetClipsQuery(command, limit, offset, searchTerm, filterType, tagSearchPrefix);

                reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false);

                while (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
                {
                    clips.Add(MapPreviewClipFromReader(reader));
                }
            }
            finally
            {
                if (reader != null)
                {
                    await reader.DisposeAsync().ConfigureAwait(false);
                }
                if (connection != null)
                {
                    await connection.DisposeAsync().ConfigureAwait(false);
                }
            }

            return clips;
        }

        public async Task<int> AddClipAsync(string content, string clipType, string? sourceApp, bool wasTrimmed)
        {
            ArgumentNullException.ThrowIfNull(content);
            ArgumentException.ThrowIfNullOrWhiteSpace(clipType);

            var contentSize = (long)Encoding.UTF8.GetByteCount(content);
            var hash = Convert.ToHexString(System.Security.Cryptography.SHA256.HashData(Encoding.UTF8.GetBytes(content)));
            string previewContent;
            if (clipType == AppConstants.ClipTypeRtf)
            {
                var plainText = RtfUtils.ToPlainText(content);
                previewContent = CreatePreview(plainText);
            }
            else
            {
                previewContent = CreatePreview(content);
            }

            long? clipId;
            SqliteConnection? connection = null;
            SqliteTransaction? transaction = null;
            try
            {
                connection = await GetOpenConnectionAsync().ConfigureAwait(false);
                transaction = (SqliteTransaction)await connection.BeginTransactionAsync().ConfigureAwait(false);

                // Get the rowid before the operation.
                long initialLastInsertRowId;
                using (var cmd = connection.CreateCommand())
                {
                    cmd.Transaction = transaction;
                    cmd.CommandText = "SELECT last_insert_rowid();";
                    initialLastInsertRowId = (long)(await cmd.ExecuteScalarAsync().ConfigureAwait(false) ?? 0L);
                }

                using (var upsertCmd = connection.CreateCommand())
                {
                    upsertCmd.Transaction = transaction;
                    upsertCmd.CommandText = @"
                    INSERT INTO clips (Content, ContentHash, PreviewContent, ClipType, SourceApp, Timestamp, IsFavorite, WasTrimmed, SizeInBytes)
                    VALUES (@Content, @ContentHash, @PreviewContent, @ClipType, @SourceApp, @Timestamp, 0, @WasTrimmed, @SizeInBytes)
                    ON CONFLICT(ContentHash) DO UPDATE SET
                        Timestamp = excluded.Timestamp,
                        SourceApp = excluded.SourceApp,
                        ClipType = excluded.ClipType,
                        WasTrimmed = excluded.WasTrimmed,
                        SizeInBytes = excluded.SizeInBytes,
                        PreviewContent = excluded.PreviewContent
                    RETURNING Id;
                ";
                    upsertCmd.Parameters.AddWithValue("@Content", content);
                    upsertCmd.Parameters.AddWithValue("@ContentHash", hash);
                    upsertCmd.Parameters.AddWithValue("@PreviewContent", previewContent);
                    upsertCmd.Parameters.AddWithValue("@ClipType", clipType);
                    upsertCmd.Parameters.AddWithValue("@SourceApp", sourceApp ?? (object)DBNull.Value);
                    upsertCmd.Parameters.AddWithValue("@Timestamp", DateTime.UtcNow.ToString("o", CultureInfo.InvariantCulture));
                    upsertCmd.Parameters.AddWithValue("@WasTrimmed", wasTrimmed ? 1 : 0);
                    upsertCmd.Parameters.AddWithValue("@SizeInBytes", contentSize);
                    clipId = (long?)(await upsertCmd.ExecuteScalarAsync().ConfigureAwait(false));
                }

                // Get the rowid after the operation. last_insert_rowid() is only updated by a true INSERT.
                long finalLastInsertRowId;
                using (var cmd = connection.CreateCommand())
                {
                    cmd.Transaction = transaction;
                    cmd.CommandText = "SELECT last_insert_rowid();";
                    finalLastInsertRowId = (long)(await cmd.ExecuteScalarAsync().ConfigureAwait(false) ?? 0L);
                }

                bool wasInserted = finalLastInsertRowId != initialLastInsertRowId;

                if (wasInserted)
                {
                    using var statCmd = connection.CreateCommand();
                    statCmd.Transaction = transaction;
                    statCmd.CommandText = "UPDATE stats SET Value = COALESCE(Value, 0) + 1 WHERE Key = 'UniqueClipsEver'";
                    await statCmd.ExecuteNonQueryAsync().ConfigureAwait(false);
                }

                await transaction.CommitAsync().ConfigureAwait(false);
            }
            finally
            {
                if (transaction != null)
                {
                    await transaction.DisposeAsync().ConfigureAwait(false);
                }
                if (connection != null)
                {
                    await connection.DisposeAsync().ConfigureAwait(false);
                }
            }


            if (clipId is null)
            {
                throw new InvalidOperationException("Failed to retrieve clip ID after upsert.");
            }

            return (int)clipId.Value;
        }

        public async Task<int> UpdateClipContentAsync(int id, string content)
        {
            ArgumentNullException.ThrowIfNull(content);

            var newHash = Convert.ToHexString(System.Security.Cryptography.SHA256.HashData(Encoding.UTF8.GetBytes(content)));
            var finalClipId = id;

            await ExecuteTransactionAsync(async (connection, transaction) =>
            {
                // Check if another clip with the same content already exists.
                const string sql = "SELECT Id, IsFavorite FROM clips WHERE ContentHash = @ContentHash";

                int? existingClipId = null;
                bool existingIsFavorite = false;

                using (var command = connection.CreateCommand())
                {
                    command.Transaction = (SqliteTransaction)transaction;
                    command.CommandText = sql;
                    command.Parameters.AddWithValue("@ContentHash", newHash);

                    SqliteDataReader? reader = null;
                    try
                    {
                        reader = await command.ExecuteReaderAsync().ConfigureAwait(false);
                        if (await reader.ReadAsync().ConfigureAwait(false))
                        {
                            existingClipId = reader.GetInt32(0);
                            existingIsFavorite = reader.GetInt64(1) == 1;
                        }
                    }
                    finally
                    {
                        if (reader != null)
                        {
                            await reader.DisposeAsync().ConfigureAwait(false);
                        }
                    }
                }

                if (existingClipId.HasValue && existingClipId.Value != id)
                {
                    // A different clip with this content already exists (MERGE case).
                    finalClipId = existingClipId.Value;

                    // 1. Get the favorite status of the clip being deleted (the one currently being edited).
                    bool currentClipWasFavorite;
                    using (var cmd = connection.CreateCommand())
                    {
                        cmd.Transaction = (SqliteTransaction)transaction;
                        cmd.CommandText = "SELECT IsFavorite FROM clips WHERE Id = @Id";
                        cmd.Parameters.AddWithValue("@Id", id);

                        SqliteDataReader? reader = null;
                        try
                        {
                            reader = await cmd.ExecuteReaderAsync().ConfigureAwait(false);
                            if (!await reader.ReadAsync().ConfigureAwait(false))
                            {
                                throw new InvalidOperationException($"Clip with ID {id} not found during content update.");
                            }
                            currentClipWasFavorite = reader.GetInt64(0) == 1;
                        }
                        finally
                        {
                            if (reader != null)
                            {
                                await reader.DisposeAsync().ConfigureAwait(false);
                            }
                        }
                    }

                    // 2. Delete the clip being edited (this one).
                    using (var deleteCmd = connection.CreateCommand())
                    {
                        deleteCmd.Transaction = (SqliteTransaction)transaction;
                        deleteCmd.CommandText = "DELETE FROM clips WHERE Id = @Id";
                        deleteCmd.Parameters.AddWithValue("@Id", id);
                        await deleteCmd.ExecuteNonQueryAsync().ConfigureAwait(false);
                    }

                    // 3. Merge favorite status, then update the kept clip.
                    var newIsFavorite = currentClipWasFavorite || existingIsFavorite;

                    using (var updateCmd = connection.CreateCommand())
                    {
                        updateCmd.Transaction = (SqliteTransaction)transaction;
                        updateCmd.CommandText = "UPDATE clips SET Timestamp = @Timestamp, IsFavorite = @IsFavorite WHERE Id = @Id";
                        updateCmd.Parameters.AddWithValue("@Timestamp", DateTime.UtcNow.ToString("o", CultureInfo.InvariantCulture));
                        updateCmd.Parameters.AddWithValue("@IsFavorite", newIsFavorite ? 1 : 0);
                        updateCmd.Parameters.AddWithValue("@Id", existingClipId.Value);
                        await updateCmd.ExecuteNonQueryAsync().ConfigureAwait(false);
                    }
                }
                else
                {
                    // No conflict. Just update the current clip (NO MERGE case).
                    var previewContent = CreatePreview(content);
                    using var updateCmd = connection.CreateCommand();
                    updateCmd.Transaction = (SqliteTransaction)transaction;
                    updateCmd.CommandText = "UPDATE clips SET Content = @Content, ContentHash = @ContentHash, PreviewContent = @PreviewContent, SizeInBytes = @SizeInBytes WHERE Id = @Id";
                    updateCmd.Parameters.AddWithValue("@Content", content);
                    updateCmd.Parameters.AddWithValue("@ContentHash", newHash);
                    updateCmd.Parameters.AddWithValue("@PreviewContent", previewContent);
                    updateCmd.Parameters.AddWithValue("@SizeInBytes", (long)Encoding.UTF8.GetByteCount(content));
                    updateCmd.Parameters.AddWithValue("@Id", id);
                    await updateCmd.ExecuteNonQueryAsync().ConfigureAwait(false);

                    finalClipId = id;
                }
            }).ConfigureAwait(false);

            return finalClipId;
        }

        public IAsyncEnumerable<string> GetAllImageClipPathsAsync()
        {
            const string sql = "SELECT Content FROM clips WHERE ClipType = @ClipType";
            var param = new SqliteParameter("@ClipType", AppConstants.ClipTypeImage);
            return QueryAsync(sql, reader => reader.GetString(0), default, param);
        }

        public IAsyncEnumerable<string> GetAllLinkClipUrlsAsync()
        {
            const string sql = "SELECT DISTINCT Content FROM clips WHERE ClipType = @ClipType";
            var param = new SqliteParameter("@ClipType", AppConstants.ClipTypeLink);
            return QueryAsync(sql, reader => reader.GetString(0), default, param);
        }

        public Task DeleteClipAsync(int id)
        {
            const string sql = "DELETE FROM clips WHERE Id = @Id";
            var param = new SqliteParameter("@Id", id);
            return ExecuteNonQueryAsync(sql, param);
        }

        public Task ToggleFavoriteAsync(int id, bool isFavorite)
        {
            const string sql = "UPDATE clips SET IsFavorite = @IsFavorite WHERE Id = @Id";
            var parameters = new[]
            {
                new SqliteParameter("@IsFavorite", isFavorite ? 1 : 0),
                new SqliteParameter("@Id", id)
            };
            return ExecuteNonQueryAsync(sql, parameters);
        }

        public Task UpdateTimestampAsync(int id)
        {
            const string sql = "UPDATE clips SET Timestamp = @Timestamp WHERE Id = @Id";
            var parameters = new[]
            {
                new SqliteParameter("@Timestamp", DateTime.UtcNow.ToString("o", CultureInfo.InvariantCulture)),
                new SqliteParameter("@Id", id)
            };
            return ExecuteNonQueryAsync(sql, parameters);
        }

        private async Task<Clip?> GetClipByHashAsync(string hash)
        {
            const string sql = "SELECT * FROM clips WHERE ContentHash = @ContentHash";
            var param = new SqliteParameter("@ContentHash", hash);

            return await QuerySingleOrDefaultAsync(sql, MapFullClipFromReader, default, param).ConfigureAwait(false);
        }

        private static Clip MapFullClipFromReader(SqliteDataReader reader)
        {
            var ordinals = new
            {
                Id = reader.GetOrdinal("Id"),
                Content = reader.GetOrdinal("Content"),
                PreviewContent = reader.GetOrdinal("PreviewContent"),
                Timestamp = reader.GetOrdinal("Timestamp"),
                ClipType = reader.GetOrdinal("ClipType"),
                SourceApp = reader.GetOrdinal("SourceApp"),
                IsFavorite = reader.GetOrdinal("IsFavorite"),
                WasTrimmed = reader.GetOrdinal("WasTrimmed"),
                SizeInBytes = reader.GetOrdinal("SizeInBytes"),
                PasteCount = reader.GetOrdinal("PasteCount"),
                Tags = reader.GetOrdinal("Tags")
            };

            return new Clip
            {
                Id = reader.GetInt32(ordinals.Id),
                Content = reader.IsDBNull(ordinals.Content) ? null : reader.GetString(ordinals.Content),
                PreviewContent = reader.IsDBNull(ordinals.PreviewContent) ? null : reader.GetString(ordinals.PreviewContent),
                Timestamp = DateTime.Parse(reader.GetString(ordinals.Timestamp), CultureInfo.InvariantCulture, DateTimeStyles.RoundtripKind).ToLocalTime(),
                ClipType = reader.GetString(ordinals.ClipType),
                SourceApp = reader.IsDBNull(ordinals.SourceApp) ? null : reader.GetString(ordinals.SourceApp),
                IsFavorite = reader.GetInt64(ordinals.IsFavorite) == 1,
                WasTrimmed = reader.GetInt64(ordinals.WasTrimmed) == 1,
                SizeInBytes = reader.GetInt64(ordinals.SizeInBytes),
                PasteCount = reader.GetInt32(ordinals.PasteCount),
                Tags = reader.IsDBNull(ordinals.Tags) ? null : reader.GetString(ordinals.Tags)
            };
        }

        public async Task<Clip?> GetClipByIdAsync(int id)
        {
            const string sql = "SELECT * FROM clips WHERE Id = @Id";
            var param = new SqliteParameter("@Id", id);

            return await QuerySingleOrDefaultAsync(sql, MapFullClipFromReader, default, param).ConfigureAwait(false);
        }

        public async Task<Clip?> GetPreviewClipByIdAsync(int id)
        {
            const string sql = "SELECT c.Id, c.Timestamp, c.ClipType, c.SourceApp, c.IsFavorite, c.WasTrimmed, c.SizeInBytes, c.PreviewContent, c.PasteCount, c.Tags, NULL as MatchContext FROM clips c WHERE Id = @Id";
            var param = new SqliteParameter("@Id", id);

            return await QuerySingleOrDefaultAsync(sql, MapPreviewClipFromReader, default, param).ConfigureAwait(false);
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

        public IAsyncEnumerable<Clip> GetAllFileBasedClipsAsync()
        {
            const string sql = "SELECT Id, Content, ClipType FROM clips WHERE ClipType LIKE 'file_%' OR ClipType = 'folder'";
            return QueryAsync(sql, reader => new Clip
            {
                Id = reader.GetInt32(0),
                Content = reader.GetString(1),
                ClipType = reader.GetString(2)
            });
        }

        public Task UpdateClipTypesAsync(Dictionary<int, string> updates)
        {
            ArgumentNullException.ThrowIfNull(updates);

            return ExecuteTransactionAsync(async (connection, transaction) =>
            {
                foreach (var update in updates)
                {
                    using var command = connection.CreateCommand();
                    command.Transaction = (SqliteTransaction)transaction;
                    command.CommandText = "UPDATE clips SET ClipType = @ClipType WHERE Id = @Id";
                    command.Parameters.AddWithValue("@Id", update.Key);
                    command.Parameters.AddWithValue("@ClipType", update.Value);
                    await command.ExecuteNonQueryAsync().ConfigureAwait(false);
                }
            });
        }

        public Task IncrementPasteCountAsync(int clipId)
        {
            const string sql = "UPDATE clips SET PasteCount = COALESCE(PasteCount, 0) + 1 WHERE Id = @Id";
            var param = new SqliteParameter("@Id", clipId);
            return ExecuteNonQueryAsync(sql, param);
        }

        public IAsyncEnumerable<Clip> GetAllClipsAsync(bool favoriteOnly)
        {
            var sql = favoriteOnly
                ? "SELECT * FROM clips WHERE IsFavorite = 1 ORDER BY Timestamp DESC"
                : "SELECT * FROM clips ORDER BY Timestamp DESC";

            return QueryAsync(sql, MapFullClipFromReader);
        }

        public async Task<int> AddClipsAsync(IEnumerable<Clip> clips)
        {
            ArgumentNullException.ThrowIfNull(clips);

            int importedCount = 0;
            await ExecuteTransactionAsync(async (connection, transaction) =>
            {
                foreach (var clip in clips)
                {
                    // Re-calculate hash and size for data integrity
                    var contentSize = (long)Encoding.UTF8.GetByteCount(clip.Content ?? string.Empty);
                    var hash = Convert.ToHexString(System.Security.Cryptography.SHA256.HashData(Encoding.UTF8.GetBytes(clip.Content ?? string.Empty)));
                    var previewContent = CreatePreview(clip.Content ?? string.Empty);

                    using var command = connection.CreateCommand();
                    command.Transaction = (SqliteTransaction)transaction;
                    command.CommandText = @"
                        INSERT OR IGNORE INTO clips (Content, ContentHash, PreviewContent, ClipType, SourceApp, Timestamp, IsFavorite, WasTrimmed, SizeInBytes, PasteCount, Tags)
                        VALUES (@Content, @ContentHash, @PreviewContent, @ClipType, @SourceApp, @Timestamp, @IsFavorite, @WasTrimmed, @SizeInBytes, @PasteCount, @Tags);
                    ";
                    command.Parameters.AddWithValue("@Content", clip.Content ?? (object)DBNull.Value);
                    command.Parameters.AddWithValue("@ContentHash", hash);
                    command.Parameters.AddWithValue("@PreviewContent", previewContent);
                    command.Parameters.AddWithValue("@ClipType", clip.ClipType);
                    command.Parameters.AddWithValue("@SourceApp", clip.SourceApp ?? (object)DBNull.Value);
                    command.Parameters.AddWithValue("@Timestamp", clip.Timestamp.ToUniversalTime().ToString("o", CultureInfo.InvariantCulture));
                    command.Parameters.AddWithValue("@IsFavorite", clip.IsFavorite ? 1 : 0);
                    command.Parameters.AddWithValue("@WasTrimmed", clip.WasTrimmed ? 1 : 0);
                    command.Parameters.AddWithValue("@SizeInBytes", contentSize);
                    command.Parameters.AddWithValue("@PasteCount", clip.PasteCount);
                    command.Parameters.AddWithValue("@Tags", clip.Tags ?? (object)DBNull.Value);

                    var rowsAffected = await command.ExecuteNonQueryAsync().ConfigureAwait(false);
                    if (rowsAffected > 0)
                    {
                        importedCount++;
                    }
                }

                if (importedCount > 0)
                {
                    using var statCmd = connection.CreateCommand();
                    statCmd.Transaction = (SqliteTransaction)transaction;
                    statCmd.CommandText = "UPDATE stats SET Value = COALESCE(Value, 0) + @Count WHERE Key = 'UniqueClipsEver'";
                    statCmd.Parameters.AddWithValue("@Count", importedCount);
                    await statCmd.ExecuteNonQueryAsync().ConfigureAwait(false);
                }
            }).ConfigureAwait(false);

            return importedCount;
        }

        public Task UpdateClipTagsAsync(int id, string tags)
        {
            const string sql = "UPDATE clips SET Tags = @Tags WHERE Id = @Id";
            var parameters = new[]
            {
                new SqliteParameter("@Tags", string.IsNullOrWhiteSpace(tags) ? (object)DBNull.Value : tags),
                new SqliteParameter("@Id", id)
            };
            return ExecuteNonQueryAsync(sql, parameters);
        }
    }
}