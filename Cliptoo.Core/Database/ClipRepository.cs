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
                    string? sourceApp = null;
                    if (!await reader.IsDBNullAsync(ordinals.SourceApp, cancellationToken).ConfigureAwait(false))
                    {
                        sourceApp = reader.GetString(ordinals.SourceApp);
                    }

                    string? previewContent = null;
                    if (!await reader.IsDBNullAsync(ordinals.PreviewContent, cancellationToken).ConfigureAwait(false))
                    {
                        previewContent = reader.GetString(ordinals.PreviewContent);
                    }

                    bool wasTrimmedDbNull = await reader.IsDBNullAsync(ordinals.WasTrimmed, cancellationToken).ConfigureAwait(false);

                    string? matchContext = null;
                    if (ordinals.MatchContext != -1 && !await reader.IsDBNullAsync(ordinals.MatchContext, cancellationToken).ConfigureAwait(false))
                    {
                        matchContext = reader.GetString(ordinals.MatchContext);
                    }

                    clips.Add(new Clip
                    {
                        Id = reader.GetInt32(ordinals.Id),
                        Timestamp = DateTime.Parse(reader.GetString(ordinals.Timestamp), CultureInfo.InvariantCulture, DateTimeStyles.RoundtripKind).ToLocalTime(),
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
            finally
            {
                if (reader != null) { await reader.DisposeAsync().ConfigureAwait(false); }
                if (command != null) { await command.DisposeAsync().ConfigureAwait(false); }
                if (connection != null) { await connection.DisposeAsync().ConfigureAwait(false); }
            }
        }

        public async Task<Clip> GetClipPreviewContentByIdAsync(int id)
        {
            SqliteConnection? connection = null;
            SqliteCommand? command = null;
            SqliteDataReader? reader = null;
            try
            {
                connection = await GetOpenConnectionAsync().ConfigureAwait(false);
                command = connection.CreateCommand();
                command.CommandText = "SELECT Id, PreviewContent, Timestamp, ClipType, SourceApp, IsPinned, WasTrimmed, SizeInBytes FROM clips WHERE Id = @Id";
                command.Parameters.AddWithValue("@Id", id);

                reader = await command.ExecuteReaderAsync().ConfigureAwait(false);
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

                    string? previewContent = null;
                    if (!await reader.IsDBNullAsync(previewContentOrdinal).ConfigureAwait(false))
                    {
                        previewContent = reader.GetString(previewContentOrdinal);
                    }

                    string? sourceApp = null;
                    if (!await reader.IsDBNullAsync(sourceAppOrdinal).ConfigureAwait(false))
                    {
                        sourceApp = reader.GetString(sourceAppOrdinal);
                    }

                    return new Clip
                    {
                        Id = reader.GetInt32(idOrdinal),
                        PreviewContent = previewContent,
                        Timestamp = DateTime.Parse(reader.GetString(timestampOrdinal), CultureInfo.InvariantCulture, DateTimeStyles.RoundtripKind).ToLocalTime(),
                        ClipType = reader.GetString(clipTypeOrdinal),
                        SourceApp = sourceApp,
                        IsPinned = reader.GetInt64(isPinnedOrdinal) == 1,
                        WasTrimmed = reader.GetInt64(wasTrimmedOrdinal) == 1,
                        SizeInBytes = reader.GetInt64(sizeInBytesOrdinal)
                    };
                }
                throw new InvalidOperationException($"Clip with ID {id} not found.");
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

                long lastId;
                long? clipId;

                SqliteCommand? upsertCmd = null;
                try
                {
                    upsertCmd = connection.CreateCommand();
                    upsertCmd.Transaction = transaction;
                    upsertCmd.CommandText = @"
                        INSERT INTO clips (Content, ContentHash, PreviewContent, ClipType, SourceApp, Timestamp, IsPinned, WasTrimmed, SizeInBytes)
                        VALUES (@Content, @ContentHash, @PreviewContent, @ClipType, @SourceApp, @Timestamp, 0, @WasTrimmed, @SizeInBytes)
                        ON CONFLICT(ContentHash) DO UPDATE SET
                            Timestamp = excluded.Timestamp,
                            SourceApp = excluded.SourceApp;
                    ";
                    upsertCmd.Parameters.AddWithValue("@Content", content);
                    upsertCmd.Parameters.AddWithValue("@ContentHash", hash);
                    upsertCmd.Parameters.AddWithValue("@PreviewContent", previewContent);
                    upsertCmd.Parameters.AddWithValue("@ClipType", clipType);
                    upsertCmd.Parameters.AddWithValue("@SourceApp", sourceApp ?? (object)DBNull.Value);
                    upsertCmd.Parameters.AddWithValue("@Timestamp", DateTime.UtcNow.ToString("o", CultureInfo.InvariantCulture));
                    upsertCmd.Parameters.AddWithValue("@WasTrimmed", wasTrimmed ? 1 : 0);
                    upsertCmd.Parameters.AddWithValue("@SizeInBytes", contentSize);
                    await upsertCmd.ExecuteNonQueryAsync().ConfigureAwait(false);
                }
                finally
                {
                    if (upsertCmd != null) { await upsertCmd.DisposeAsync().ConfigureAwait(false); }
                }

                SqliteCommand? lastIdCmd = null;
                try
                {
                    lastIdCmd = connection.CreateCommand();
                    lastIdCmd.Transaction = transaction;
                    lastIdCmd.CommandText = "SELECT last_insert_rowid();";
                    lastId = (long)(await lastIdCmd.ExecuteScalarAsync().ConfigureAwait(false) ?? 0L);
                }
                finally
                {
                    if (lastIdCmd != null) { await lastIdCmd.DisposeAsync().ConfigureAwait(false); }
                }


                if (lastId > 0)
                {
                    SqliteCommand? statCmd = null;
                    try
                    {
                        statCmd = connection.CreateCommand();
                        statCmd.Transaction = transaction;
                        statCmd.CommandText = "UPDATE stats SET Value = COALESCE(Value, 0) + 1 WHERE Key = 'TotalClipsEver'";
                        await statCmd.ExecuteNonQueryAsync().ConfigureAwait(false);
                    }
                    finally
                    {
                        if (statCmd != null) { await statCmd.DisposeAsync().ConfigureAwait(false); }
                    }
                }

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

                return (int)clipId;
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

        public async IAsyncEnumerable<string> GetAllImageClipPathsAsync()
        {
            SqliteConnection? connection = null;
            SqliteCommand? command = null;
            SqliteDataReader? reader = null;
            try
            {
                connection = await GetOpenConnectionAsync().ConfigureAwait(false);
                command = connection.CreateCommand();
                command.CommandText = "SELECT Content FROM clips WHERE ClipType = @ClipType";
                command.Parameters.AddWithValue("@ClipType", AppConstants.ClipTypes.Image);

                reader = await command.ExecuteReaderAsync().ConfigureAwait(false);
                while (await reader.ReadAsync().ConfigureAwait(false))
                {
                    yield return reader.GetString(0);
                }
            }
            finally
            {
                if (reader != null) { await reader.DisposeAsync().ConfigureAwait(false); }
                if (command != null) { await command.DisposeAsync().ConfigureAwait(false); }
                if (connection != null) { await connection.DisposeAsync().ConfigureAwait(false); }
            }
        }

        public async IAsyncEnumerable<string> GetAllLinkClipUrlsAsync()
        {
            SqliteConnection? connection = null;
            SqliteCommand? command = null;
            SqliteDataReader? reader = null;
            try
            {
                connection = await GetOpenConnectionAsync().ConfigureAwait(false);
                command = connection.CreateCommand();
                command.CommandText = "SELECT DISTINCT Content FROM clips WHERE ClipType = @ClipType";
                command.Parameters.AddWithValue("@ClipType", AppConstants.ClipTypes.Link);

                reader = await command.ExecuteReaderAsync().ConfigureAwait(false);
                while (await reader.ReadAsync().ConfigureAwait(false))
                {
                    yield return reader.GetString(0);
                }
            }
            finally
            {
                if (reader != null) { await reader.DisposeAsync().ConfigureAwait(false); }
                if (command != null) { await command.DisposeAsync().ConfigureAwait(false); }
                if (connection != null) { await connection.DisposeAsync().ConfigureAwait(false); }
            }
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

        public async Task<Clip?> GetClipByIdAsync(int id)
        {
            SqliteConnection? connection = null;
            SqliteCommand? command = null;
            SqliteDataReader? reader = null;
            try
            {
                connection = await GetOpenConnectionAsync().ConfigureAwait(false);
                command = connection.CreateCommand();
                command.CommandText = "SELECT * FROM clips WHERE Id = @Id";
                command.Parameters.AddWithValue("@Id", id);

                reader = await command.ExecuteReaderAsync().ConfigureAwait(false);
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

                    string? content = null;
                    if (!await reader.IsDBNullAsync(contentOrdinal).ConfigureAwait(false))
                    {
                        content = reader.GetString(contentOrdinal);
                    }

                    string? previewContent = null;
                    if (!await reader.IsDBNullAsync(previewContentOrdinal).ConfigureAwait(false))
                    {
                        previewContent = reader.GetString(previewContentOrdinal);
                    }

                    string? sourceApp = null;
                    if (!await reader.IsDBNullAsync(sourceAppOrdinal).ConfigureAwait(false))
                    {
                        sourceApp = reader.GetString(sourceAppOrdinal);
                    }

                    return new Clip
                    {
                        Id = reader.GetInt32(idOrdinal),
                        Content = content,
                        PreviewContent = previewContent,
                        Timestamp = DateTime.Parse(reader.GetString(timestampOrdinal), CultureInfo.InvariantCulture, DateTimeStyles.RoundtripKind).ToLocalTime(),
                        ClipType = reader.GetString(clipTypeOrdinal),
                        SourceApp = sourceApp,
                        IsPinned = reader.GetInt64(isPinnedOrdinal) == 1,
                        WasTrimmed = reader.GetInt64(wasTrimmedOrdinal) == 1,
                        SizeInBytes = reader.GetInt64(sizeInBytesOrdinal)
                    };
                }
                return null;
            }
            finally
            {
                if (reader != null) { await reader.DisposeAsync().ConfigureAwait(false); }
                if (command != null) { await command.DisposeAsync().ConfigureAwait(false); }
                if (connection != null) { await connection.DisposeAsync().ConfigureAwait(false); }
            }
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

        public async IAsyncEnumerable<Clip> GetAllFileBasedClipsAsync()
        {
            SqliteConnection? connection = null;
            SqliteCommand? command = null;
            SqliteDataReader? reader = null;
            try
            {
                connection = await GetOpenConnectionAsync().ConfigureAwait(false);
                command = connection.CreateCommand();
                command.CommandText = "SELECT Id, Content, ClipType FROM clips WHERE ClipType LIKE 'file_%' OR ClipType = 'folder'";

                reader = await command.ExecuteReaderAsync().ConfigureAwait(false);
                while (await reader.ReadAsync().ConfigureAwait(false))
                {
                    yield return new Clip
                    {
                        Id = reader.GetInt32(0),
                        Content = reader.GetString(1),
                        ClipType = reader.GetString(2)
                    };
                }
            }
            finally
            {
                if (reader != null) { await reader.DisposeAsync().ConfigureAwait(false); }
                if (command != null) { await command.DisposeAsync().ConfigureAwait(false); }
                if (connection != null) { await connection.DisposeAsync().ConfigureAwait(false); }
            }
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