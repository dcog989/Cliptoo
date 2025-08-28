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

            SqliteConnection? connection = null;
            SqliteCommand? command = null;
            try
            {
                connection = await GetOpenConnectionAsync().ConfigureAwait(false);
                var contentSize = (long)System.Text.Encoding.UTF8.GetByteCount(content);
                var hash = Convert.ToHexString(System.Security.Cryptography.SHA256.HashData(System.Text.Encoding.UTF8.GetBytes(content)));

                command = connection.CreateCommand();
                command.CommandText = "SELECT Id FROM clips WHERE ContentHash = @Hash";
                command.Parameters.AddWithValue("@Hash", hash);
                var existingIdObj = await command.ExecuteScalarAsync().ConfigureAwait(false);
                if (existingIdObj != null)
                {
                    var existingId = Convert.ToInt32(existingIdObj, CultureInfo.InvariantCulture);
                    await command.DisposeAsync().ConfigureAwait(false);

                    command = connection.CreateCommand();
                    command.CommandText = "UPDATE clips SET Timestamp = @Timestamp, SourceApp = @SourceApp WHERE Id = @Id";
                    command.Parameters.AddWithValue("@Timestamp", DateTime.UtcNow.ToString("o", CultureInfo.InvariantCulture));
                    command.Parameters.AddWithValue("@SourceApp", sourceApp ?? (object)DBNull.Value);
                    command.Parameters.AddWithValue("@Id", existingId);
                    await command.ExecuteNonQueryAsync().ConfigureAwait(false);
                    return existingId;
                }
                await command.DisposeAsync().ConfigureAwait(false);

                var previewContent = CreatePreview(content);
                command = connection.CreateCommand();
                command.CommandText = @"
                        INSERT INTO clips (Content, ContentHash, PreviewContent, ClipType, SourceApp, Timestamp, IsPinned, WasTrimmed, SizeInBytes)
                        VALUES (@Content, @ContentHash, @PreviewContent, @ClipType, @SourceApp, @Timestamp, 0, @WasTrimmed, @SizeInBytes);
                        SELECT last_insert_rowid();";
                command.Parameters.AddWithValue("@Content", content);
                command.Parameters.AddWithValue("@ContentHash", hash);
                command.Parameters.AddWithValue("@PreviewContent", previewContent);
                command.Parameters.AddWithValue("@ClipType", clipType);
                command.Parameters.AddWithValue("@SourceApp", sourceApp ?? (object)DBNull.Value);
                command.Parameters.AddWithValue("@Timestamp", DateTime.UtcNow.ToString("o", CultureInfo.InvariantCulture));
                command.Parameters.AddWithValue("@WasTrimmed", wasTrimmed ? 1 : 0);
                command.Parameters.AddWithValue("@SizeInBytes", contentSize);
                var newId = (long)(await command.ExecuteScalarAsync().ConfigureAwait(false) ?? -1L);
                await command.DisposeAsync().ConfigureAwait(false);

                command = connection.CreateCommand();
                command.CommandText = "UPDATE stats SET Value = COALESCE(Value, 0) + 1 WHERE Key = 'TotalClipsEver'";
                await command.ExecuteNonQueryAsync().ConfigureAwait(false);
                return (int)newId;
            }
            finally
            {
                if (command != null) { await command.DisposeAsync().ConfigureAwait(false); }
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

        public async Task<List<Clip>> GetAllFileBasedClipsAsync()
        {
            var clips = new List<Clip>();
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
                    clips.Add(new Clip
                    {
                        Id = reader.GetInt32(0),
                        Content = reader.GetString(1),
                        ClipType = reader.GetString(2)
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