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

        [SuppressMessage("Microsoft.Security", "CA2100:Review SQL queries for security vulnerabilities", Justification = "OrderBy clause is constructed from hardcoded strings, not user input.")]
        public async Task<List<Clip>> GetClipsAsync(uint limit, uint offset, string searchTerm, string filterType, CancellationToken cancellationToken)
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

                ClipQueryBuilder.BuildGetClipsQuery(command, limit, offset, searchTerm, filterType);

                reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false);

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
                    clips.Add(new Clip
                    {
                        Id = reader.GetInt32(ordinals.Id),
                        Timestamp = DateTime.Parse(reader.GetString(ordinals.Timestamp), CultureInfo.InvariantCulture, DateTimeStyles.RoundtripKind).ToLocalTime(),
                        ClipType = reader.GetString(ordinals.ClipType),
                        SourceApp = await reader.IsDBNullAsync(ordinals.SourceApp, cancellationToken).ConfigureAwait(false) ? null : reader.GetString(ordinals.SourceApp),
                        IsPinned = reader.GetInt64(ordinals.IsPinned) == 1,
                        WasTrimmed = reader.GetInt64(ordinals.WasTrimmed) == 1,
                        SizeInBytes = reader.GetInt64(ordinals.SizeInBytes),
                        PreviewContent = await reader.IsDBNullAsync(ordinals.PreviewContent, cancellationToken).ConfigureAwait(false) ? null : reader.GetString(ordinals.PreviewContent),
                        MatchContext = ordinals.MatchContext != -1 && !await reader.IsDBNullAsync(ordinals.MatchContext, cancellationToken).ConfigureAwait(false) ? reader.GetString(ordinals.MatchContext) : null
                    });
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

                long changes;

                SqliteCommand? insertCmd = null;
                try
                {
                    insertCmd = connection.CreateCommand();
                    insertCmd.Transaction = transaction;
                    insertCmd.CommandText = @"
                INSERT OR IGNORE INTO clips (Content, ContentHash, PreviewContent, ClipType, SourceApp, Timestamp, IsPinned, WasTrimmed, SizeInBytes)
                VALUES (@Content, @ContentHash, @PreviewContent, @ClipType, @SourceApp, @Timestamp, 0, @WasTrimmed, @SizeInBytes);
            ";
                    insertCmd.Parameters.AddWithValue("@Content", content);
                    insertCmd.Parameters.AddWithValue("@ContentHash", hash);
                    insertCmd.Parameters.AddWithValue("@PreviewContent", previewContent);
                    insertCmd.Parameters.AddWithValue("@ClipType", clipType);
                    insertCmd.Parameters.AddWithValue("@SourceApp", sourceApp ?? (object)DBNull.Value);
                    insertCmd.Parameters.AddWithValue("@Timestamp", DateTime.UtcNow.ToString("o", CultureInfo.InvariantCulture));
                    insertCmd.Parameters.AddWithValue("@WasTrimmed", wasTrimmed ? 1 : 0);
                    insertCmd.Parameters.AddWithValue("@SizeInBytes", contentSize);
                    await insertCmd.ExecuteNonQueryAsync().ConfigureAwait(false);

                    using var changesCmd = connection.CreateCommand();
                    changesCmd.Transaction = transaction;
                    changesCmd.CommandText = "SELECT changes();";
                    changes = (long)(await changesCmd.ExecuteScalarAsync().ConfigureAwait(false) ?? 0L);
                }
                finally
                {
                    if (insertCmd != null) { await insertCmd.DisposeAsync().ConfigureAwait(false); }
                }

                if (changes > 0)
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
                else
                {
                    SqliteCommand? updateCmd = null;
                    try
                    {
                        updateCmd = connection.CreateCommand();
                        updateCmd.Transaction = transaction;
                        updateCmd.CommandText = @"
                    UPDATE clips SET Timestamp = @Timestamp, SourceApp = @SourceApp
                    WHERE ContentHash = @ContentHash;
                ";
                        updateCmd.Parameters.AddWithValue("@Timestamp", DateTime.UtcNow.ToString("o", CultureInfo.InvariantCulture));
                        updateCmd.Parameters.AddWithValue("@SourceApp", sourceApp ?? (object)DBNull.Value);
                        updateCmd.Parameters.AddWithValue("@ContentHash", hash);
                        await updateCmd.ExecuteNonQueryAsync().ConfigureAwait(false);
                    }
                    finally
                    {
                        if (updateCmd != null) { await updateCmd.DisposeAsync().ConfigureAwait(false); }
                    }
                }

                long? clipId;
                SqliteCommand? selectCmd = null;
                try
                {
                    selectCmd = connection.CreateCommand();
                    selectCmd.Transaction = transaction;
                    selectCmd.CommandText = "SELECT Id FROM clips WHERE ContentHash = @ContentHash";
                    selectCmd.Parameters.AddWithValue("@ContentHash", hash);
                    clipId = (long?)(await selectCmd.ExecuteScalarAsync().ConfigureAwait(false));
                }
                finally
                {
                    if (selectCmd != null) { await selectCmd.DisposeAsync().ConfigureAwait(false); }
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

        public Task UpdateClipContentAsync(int id, string content)
        {
            ArgumentNullException.ThrowIfNull(content);
            var previewContent = CreatePreview(content);
            var hash = Convert.ToHexString(System.Security.Cryptography.SHA256.HashData(System.Text.Encoding.UTF8.GetBytes(content)));
            var sql = "UPDATE clips SET Content = @Content, ContentHash = @ContentHash, PreviewContent = @PreviewContent, SizeInBytes = @SizeInBytes WHERE Id = @Id";
            var parameters = new[]
            {
                new SqliteParameter("@Content", content),
                new SqliteParameter("@ContentHash", hash),
                new SqliteParameter("@PreviewContent", previewContent),
                new SqliteParameter("@SizeInBytes", (long)System.Text.Encoding.UTF8.GetByteCount(content)),
                new SqliteParameter("@Id", id)
            };
            return ExecuteNonQueryAsync(sql, parameters);
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

        public Task TogglePinAsync(int id, bool isPinned)
        {
            var sql = "UPDATE clips SET IsPinned = @IsPinned WHERE Id = @Id";
            var parameters = new[]
            {
                new SqliteParameter("@IsPinned", isPinned ? 1 : 0),
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
                IsPinned = reader.GetInt64(reader.GetOrdinal("IsPinned")) == 1,
                WasTrimmed = reader.GetInt64(reader.GetOrdinal("WasTrimmed")) == 1,
                SizeInBytes = reader.GetInt64(reader.GetOrdinal("SizeInBytes"))
            };
        }

        public async Task<Clip?> GetClipByIdAsync(int id)
        {
            var sql = "SELECT * FROM clips WHERE Id = @Id";
            var param = new SqliteParameter("@Id", id);

            return await QuerySingleOrDefaultAsync(sql, MapFullClipFromReader, default, param).ConfigureAwait(false);
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
    }
}