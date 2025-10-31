using System;
using System.Collections.Generic;
using System.Diagnostics.CodeAnalysis;
using System.Globalization;
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

        [SuppressMessage("Microsoft.Security", "CA2100:Review SQL queries for security vulnerabilities", Justification = "OrderBy clause is constructed from hardcoded strings, not user input.")]
        public async Task<List<Clip>> GetClipsAsync(uint limit, uint offset, string searchTerm, string filterType, string tagSearchPrefix, CancellationToken cancellationToken)
        {
            ArgumentNullException.ThrowIfNull(searchTerm);
            ArgumentNullException.ThrowIfNull(filterType);

            var clips = new List<Clip>();
            SqliteConnection? connection = null;
            SqliteCommand? command = null;
            SqliteDataReader? reader = null;
            try
            {
                connection = await GetOpenConnectionAsync().ConfigureAwait(false);
                command = connection.CreateCommand();

                ClipQueryBuilder.BuildGetClipsQuery(command, limit, offset, searchTerm, filterType, tagSearchPrefix);

                reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false);

                while (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
                {
                    clips.Add(MapPreviewClipFromReader(reader));
                }
                return clips;
            }
            finally
            {
                if (reader != null) { await reader.DisposeAsync().ConfigureAwait(false); }
                if (command != null) { await command.DisposeAsync().ConfigureAwait(false); }
                if (connection != null) { await connection.DisposeAsync().ConfigureAwait(false); }
            }
        }

        public async Task<int> AddClipAsync(string content, string clipType, string? sourceApp, bool wasTrimmed)
        {
            ArgumentNullException.ThrowIfNull(content);

            var contentSize = (long)System.Text.Encoding.UTF8.GetByteCount(content);
            var hash = Convert.ToHexString(System.Security.Cryptography.SHA256.HashData(System.Text.Encoding.UTF8.GetBytes(content)));
            var previewContent = CreatePreview(content);

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

                long? clipId;
                SqliteCommand? upsertCmd = null;
                try
                {
                    upsertCmd = connection.CreateCommand();
                    upsertCmd.Transaction = transaction;
                    upsertCmd.CommandText = @"
                        INSERT INTO clips (Content, ContentHash, PreviewContent, ClipType, SourceApp, Timestamp, IsFavorite, WasTrimmed, SizeInBytes)
                        VALUES (@Content, @ContentHash, @PreviewContent, @ClipType, @SourceApp, @Timestamp, 0, @WasTrimmed, @SizeInBytes)
                        ON CONFLICT(ContentHash) DO UPDATE SET
                            Timestamp = excluded.Timestamp,
                            SourceApp = excluded.SourceApp
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
                finally
                {
                    if (upsertCmd != null) { await upsertCmd.DisposeAsync().ConfigureAwait(false); }
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
                    SqliteCommand? statCmd = null;
                    try
                    {
                        statCmd = connection.CreateCommand();
                        statCmd.Transaction = transaction;
                        statCmd.CommandText = "UPDATE stats SET Value = COALESCE(Value, 0) + 1 WHERE Key = 'UniqueClipsEver'";
                        await statCmd.ExecuteNonQueryAsync().ConfigureAwait(false);
                    }
                    finally
                    {
                        if (statCmd != null) { await statCmd.DisposeAsync().ConfigureAwait(false); }
                    }
                }

                await transaction.CommitAsync().ConfigureAwait(false);

                if (clipId is null)
                {
                    throw new InvalidOperationException("Failed to retrieve clip ID after upsert.");
                }

                return (int)clipId.Value;
            }
            finally
            {
                if (transaction != null) { await transaction.DisposeAsync().ConfigureAwait(false); }
                if (connection != null) { await connection.DisposeAsync().ConfigureAwait(false); }
            }
        }

        [SuppressMessage("Microsoft.Security", "CA2100:Review SQL queries for security vulnerabilities", Justification = "The SQL query is hardcoded and uses parameters, making it safe from injection.")]
        public async Task UpdateClipContentAsync(int id, string content)
        {
            ArgumentNullException.ThrowIfNull(content);
            var newHash = Convert.ToHexString(System.Security.Cryptography.SHA256.HashData(System.Text.Encoding.UTF8.GetBytes(content)));

            await ExecuteTransactionAsync(async (connection, transaction) =>
            {
                // Check if another clip with the same content already exists.
                var sql = "SELECT Id FROM clips WHERE ContentHash = @ContentHash";
                var param = new SqliteParameter("@ContentHash", newHash);

                int? existingClipId = null;
                using (var command = connection.CreateCommand())
                {
                    command.Transaction = (SqliteTransaction)transaction;
                    command.CommandText = sql;
                    command.Parameters.Add(param);
                    var result = await command.ExecuteScalarAsync().ConfigureAwait(false);
                    if (result is not null and not DBNull)
                    {
                        existingClipId = Convert.ToInt32(result, CultureInfo.InvariantCulture);
                    }
                }

                if (existingClipId.HasValue && existingClipId.Value != id)
                {
                    // A different clip with this content already exists.
                    // Delete the clip being edited and update the timestamp of the existing one.
                    using (var deleteCmd = connection.CreateCommand())
                    {
                        deleteCmd.Transaction = (SqliteTransaction)transaction;
                        deleteCmd.CommandText = "DELETE FROM clips WHERE Id = @Id";
                        deleteCmd.Parameters.AddWithValue("@Id", id);
                        await deleteCmd.ExecuteNonQueryAsync().ConfigureAwait(false);
                    }
                    using (var updateCmd = connection.CreateCommand())
                    {
                        updateCmd.Transaction = (SqliteTransaction)transaction;
                        updateCmd.CommandText = "UPDATE clips SET Timestamp = @Timestamp WHERE Id = @Id";
                        updateCmd.Parameters.AddWithValue("@Timestamp", DateTime.UtcNow.ToString("o", CultureInfo.InvariantCulture));
                        updateCmd.Parameters.AddWithValue("@Id", existingClipId.Value);
                        await updateCmd.ExecuteNonQueryAsync().ConfigureAwait(false);
                    }
                }
                else
                {
                    // No conflict. Just update the current clip.
                    var previewContent = CreatePreview(content);
                    using (var updateCmd = connection.CreateCommand())
                    {
                        updateCmd.Transaction = (SqliteTransaction)transaction;
                        updateCmd.CommandText = "UPDATE clips SET Content = @Content, ContentHash = @ContentHash, PreviewContent = @PreviewContent, SizeInBytes = @SizeInBytes WHERE Id = @Id";
                        updateCmd.Parameters.AddWithValue("@Content", content);
                        updateCmd.Parameters.AddWithValue("@ContentHash", newHash);
                        updateCmd.Parameters.AddWithValue("@PreviewContent", previewContent);
                        updateCmd.Parameters.AddWithValue("@SizeInBytes", (long)System.Text.Encoding.UTF8.GetByteCount(content));
                        updateCmd.Parameters.AddWithValue("@Id", id);
                        await updateCmd.ExecuteNonQueryAsync().ConfigureAwait(false);
                    }
                }
            }).ConfigureAwait(false);
        }

        public IAsyncEnumerable<string> GetAllImageClipPathsAsync()
        {
            var sql = "SELECT Content FROM clips WHERE ClipType = @ClipType";
            var param = new SqliteParameter("@ClipType", AppConstants.ClipTypes.Image);
            return QueryAsync(sql, reader => reader.GetString(0), default, param);
        }

        public IAsyncEnumerable<string> GetAllLinkClipUrlsAsync()
        {
            var sql = "SELECT DISTINCT Content FROM clips WHERE ClipType = @ClipType";
            var param = new SqliteParameter("@ClipType", AppConstants.ClipTypes.Link);
            return QueryAsync(sql, reader => reader.GetString(0), default, param);
        }

        public Task DeleteClipAsync(int id)
        {
            var sql = "DELETE FROM clips WHERE Id = @Id";
            var param = new SqliteParameter("@Id", id);
            return ExecuteNonQueryAsync(sql, param);
        }

        public Task ToggleFavoriteAsync(int id, bool isFavorite)
        {
            var sql = "UPDATE clips SET IsFavorite = @IsFavorite WHERE Id = @Id";
            var parameters = new[]
            {
                new SqliteParameter("@IsFavorite", isFavorite ? 1 : 0),
                new SqliteParameter("@Id", id)
            };
            return ExecuteNonQueryAsync(sql, parameters);
        }

        public Task UpdateTimestampAsync(int id)
        {
            var sql = "UPDATE clips SET Timestamp = @Timestamp WHERE Id = @Id";
            var parameters = new[]
            {
                new SqliteParameter("@Timestamp", DateTime.UtcNow.ToString("o", CultureInfo.InvariantCulture)),
                new SqliteParameter("@Id", id)
            };
            return ExecuteNonQueryAsync(sql, parameters);
        }

        private async Task<Clip?> GetClipByHashAsync(string hash)
        {
            var sql = "SELECT * FROM clips WHERE ContentHash = @ContentHash";
            var param = new SqliteParameter("@ContentHash", hash);

            return await QuerySingleOrDefaultAsync(sql, MapFullClipFromReader, default, param).ConfigureAwait(false);
        }

        private static Clip MapFullClipFromReader(SqliteDataReader reader)
        {
            return new Clip
            {
                Id = reader.GetInt32(reader.GetOrdinal("Id")),
                Content = reader.IsDBNull(reader.GetOrdinal("Content")) ? null : reader.GetString(reader.GetOrdinal("Content")),
                PreviewContent = reader.IsDBNull(reader.GetOrdinal("PreviewContent")) ? null : reader.GetString(reader.GetOrdinal("PreviewContent")),
                Timestamp = DateTime.Parse(reader.GetString(reader.GetOrdinal("Timestamp")), CultureInfo.InvariantCulture, DateTimeStyles.RoundtripKind).ToLocalTime(),
                ClipType = reader.GetString(reader.GetOrdinal("ClipType")),
                SourceApp = reader.IsDBNull(reader.GetOrdinal("SourceApp")) ? null : reader.GetString(reader.GetOrdinal("SourceApp")),
                IsFavorite = reader.GetInt64(reader.GetOrdinal("IsFavorite")) == 1,
                WasTrimmed = reader.GetInt64(reader.GetOrdinal("WasTrimmed")) == 1,
                SizeInBytes = reader.GetInt64(reader.GetOrdinal("SizeInBytes")),
                PasteCount = reader.GetInt32(reader.GetOrdinal("PasteCount")),
                Tags = reader.IsDBNull(reader.GetOrdinal("Tags")) ? null : reader.GetString(reader.GetOrdinal("Tags"))
            };
        }

        public async Task<Clip?> GetClipByIdAsync(int id)
        {
            var sql = "SELECT * FROM clips WHERE Id = @Id";
            var param = new SqliteParameter("@Id", id);

            return await QuerySingleOrDefaultAsync(sql, MapFullClipFromReader, default, param).ConfigureAwait(false);
        }

        public async Task<Clip?> GetPreviewClipByIdAsync(int id)
        {
            var sql = "SELECT c.Id, c.Timestamp, c.ClipType, c.SourceApp, c.IsFavorite, c.WasTrimmed, c.SizeInBytes, c.PreviewContent, c.PasteCount, c.Tags, NULL as MatchContext FROM clips c WHERE Id = @Id";
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
            var sql = "SELECT Id, Content, ClipType FROM clips WHERE ClipType LIKE 'file_%' OR ClipType = 'folder'";
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
                    SqliteCommand? command = null;
                    try
                    {
                        command = connection.CreateCommand();
                        command.Transaction = (SqliteTransaction)transaction;
                        command.CommandText = "UPDATE clips SET ClipType = @ClipType WHERE Id = @Id";
                        command.Parameters.AddWithValue("@Id", update.Key);
                        command.Parameters.AddWithValue("@ClipType", update.Value);
                        await command.ExecuteNonQueryAsync().ConfigureAwait(false);
                    }
                    finally
                    {
                        if (command != null)
                        {
                            await command.DisposeAsync().ConfigureAwait(false);
                        }
                    }
                }
            });
        }

        public Task IncrementPasteCountAsync(int clipId)
        {
            var sql = "UPDATE clips SET PasteCount = COALESCE(PasteCount, 0) + 1 WHERE Id = @Id";
            var param = new SqliteParameter("@Id", clipId);
            return ExecuteNonQueryAsync(sql, param);
        }

        public IAsyncEnumerable<Clip> GetAllClipsAsync(bool favoriteOnly)
        {
            var sql = new StringBuilder("SELECT * FROM clips");
            if (favoriteOnly)
            {
                sql.Append(" WHERE IsFavorite = 1");
            }
            sql.Append(" ORDER BY Timestamp DESC");

            return QueryAsync(sql.ToString(), MapFullClipFromReader);
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
                    var contentSize = (long)Encoding.UTF8.GetByteCount(clip.Content ?? "");
                    var hash = Convert.ToHexString(System.Security.Cryptography.SHA256.HashData(Encoding.UTF8.GetBytes(clip.Content ?? "")));
                    var previewContent = CreatePreview(clip.Content ?? "");

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
            var sql = "UPDATE clips SET Tags = @Tags WHERE Id = @Id";
            var parameters = new[]
            {
                new SqliteParameter("@Tags", string.IsNullOrWhiteSpace(tags) ? (object)DBNull.Value : tags),
                new SqliteParameter("@Id", id)
            };
            return ExecuteNonQueryAsync(sql, parameters);
        }
    }
}