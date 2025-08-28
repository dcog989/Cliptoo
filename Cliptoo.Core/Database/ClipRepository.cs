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
            try
            {
                connection = await GetOpenConnectionAsync().ConfigureAwait(false);
                var contentSize = (long)System.Text.Encoding.UTF8.GetByteCount(content);
                var hash = Convert.ToHexString(System.Security.Cryptography.SHA256.HashData(System.Text.Encoding.UTF8.GetBytes(content)));

                SqliteCommand? selectCmd = null;
                try
                {
                    selectCmd = connection.CreateCommand();
                    selectCmd.CommandText = "SELECT Id FROM clips WHERE ContentHash = @Hash";
                    selectCmd.Parameters.AddWithValue("@Hash", hash);
                    var existingIdObj = await selectCmd.ExecuteScalarAsync().ConfigureAwait(false);
                    if (existingIdObj != null)
                    {
                        var existingId = Convert.ToInt32(existingIdObj, CultureInfo.InvariantCulture);
                        SqliteCommand? updateCmd = null;
                        try
                        {
                            updateCmd = connection.CreateCommand();
                            updateCmd.CommandText = "UPDATE clips SET Timestamp = @Timestamp, SourceApp = @SourceApp WHERE Id = @Id";
                            updateCmd.Parameters.AddWithValue("@Timestamp", DateTime.UtcNow.ToString("o", CultureInfo.InvariantCulture));
                            updateCmd.Parameters.AddWithValue("@SourceApp", sourceApp ?? (object)DBNull.Value);
                            updateCmd.Parameters.AddWithValue("@Id", existingId);
                            await updateCmd.ExecuteNonQueryAsync().ConfigureAwait(false);
                            return existingId;
                        }
                        finally
                        {
                            if (updateCmd != null) { await updateCmd.DisposeAsync().ConfigureAwait(false); }
                        }
                    }
                }
                finally
                {
                    if (selectCmd != null) { await selectCmd.DisposeAsync().ConfigureAwait(false); }
                }

                var previewContent = CreatePreview(content);
                long newId;
                SqliteCommand? insertCmd = null;
                try
                {
                    insertCmd = connection.CreateCommand();
                    insertCmd.CommandText = @"
                        INSERT INTO clips (Content, ContentHash, PreviewContent, ClipType, SourceApp, Timestamp, IsPinned, WasTrimmed, SizeInBytes)
                        VALUES (@Content, @ContentHash, @PreviewContent, @ClipType, @SourceApp, @Timestamp, 0, @WasTrimmed, @SizeInBytes);
                        SELECT last_insert_rowid();";
                    insertCmd.Parameters.AddWithValue("@Content", content);
                    insertCmd.Parameters.AddWithValue("@ContentHash", hash);
                    insertCmd.Parameters.AddWithValue("@PreviewContent", previewContent);
                    insertCmd.Parameters.AddWithValue("@ClipType", clipType);
                    insertCmd.Parameters.AddWithValue("@SourceApp", sourceApp ?? (object)DBNull.Value);
                    insertCmd.Parameters.AddWithValue("@Timestamp", DateTime.UtcNow.ToString("o", CultureInfo.InvariantCulture));
                    insertCmd.Parameters.AddWithValue("@WasTrimmed", wasTrimmed ? 1 : 0);
                    insertCmd.Parameters.AddWithValue("@SizeInBytes", contentSize);
                    newId = (long)(await insertCmd.ExecuteScalarAsync().ConfigureAwait(false) ?? -1L);
                }
                finally
                {
                    if (insertCmd != null) { await insertCmd.DisposeAsync().ConfigureAwait(false); }
                }

                SqliteCommand? updateTotalCmd = null;
                try
                {
                    updateTotalCmd = connection.CreateCommand();
                    updateTotalCmd.CommandText = "UPDATE stats SET Value = COALESCE(Value, 0) + 1 WHERE Key = 'TotalClipsEver'";
                    await updateTotalCmd.ExecuteNonQueryAsync().ConfigureAwait(false);
                }
                finally
                {
                    if (updateTotalCmd != null) { await updateTotalCmd.DisposeAsync().ConfigureAwait(false); }
                }

                return (int)newId;
            }
            finally
            {
                if (connection != null) { await connection.DisposeAsync().ConfigureAwait(false); }
            }
        }

        public async Task UpdateClipContentAsync(int id, string content)
        {
            ArgumentNullException.ThrowIfNull(content);
            SqliteConnection? connection = null;
            SqliteCommand? command = null;
            try
            {
                connection = await GetOpenConnectionAsync().ConfigureAwait(false);
                command = connection.CreateCommand();
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
            finally
            {
                if (command != null) { await command.DisposeAsync().ConfigureAwait(false); }
                if (connection != null) { await connection.DisposeAsync().ConfigureAwait(false); }
            }
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

        public async Task DeleteClipAsync(int id)
        {
            SqliteConnection? connection = null;
            SqliteCommand? command = null;
            try
            {
                connection = await GetOpenConnectionAsync().ConfigureAwait(false);
                command = connection.CreateCommand();
                command.CommandText = "DELETE FROM clips WHERE Id = @Id";
                command.Parameters.AddWithValue("@Id", id);
                await command.ExecuteNonQueryAsync().ConfigureAwait(false);
            }
            finally
            {
                if (command != null) { await command.DisposeAsync().ConfigureAwait(false); }
                if (connection != null) { await connection.DisposeAsync().ConfigureAwait(false); }
            }
        }

        public async Task TogglePinAsync(int id, bool isPinned)
        {
            SqliteConnection? connection = null;
            SqliteCommand? command = null;
            try
            {
                connection = await GetOpenConnectionAsync().ConfigureAwait(false);
                command = connection.CreateCommand();
                command.CommandText = "UPDATE clips SET IsPinned = @IsPinned WHERE Id = @Id";
                command.Parameters.AddWithValue("@IsPinned", isPinned ? 1 : 0);
                command.Parameters.AddWithValue("@Id", id);
                await command.ExecuteNonQueryAsync().ConfigureAwait(false);
            }
            finally
            {
                if (command != null) { await command.DisposeAsync().ConfigureAwait(false); }
                if (connection != null) { await connection.DisposeAsync().ConfigureAwait(false); }
            }
        }

        public async Task UpdateTimestampAsync(int id)
        {
            SqliteConnection? connection = null;
            SqliteCommand? command = null;
            try
            {
                connection = await GetOpenConnectionAsync().ConfigureAwait(false);
                command = connection.CreateCommand();
                command.CommandText = "UPDATE clips SET Timestamp = @Timestamp WHERE Id = @Id";
                command.Parameters.AddWithValue("@Timestamp", DateTime.UtcNow.ToString("o", CultureInfo.InvariantCulture));
                command.Parameters.AddWithValue("@Id", id);
                await command.ExecuteNonQueryAsync().ConfigureAwait(false);
            }
            finally
            {
                if (command != null) { await command.DisposeAsync().ConfigureAwait(false); }
                if (connection != null) { await connection.DisposeAsync().ConfigureAwait(false); }
            }
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

        public async Task UpdateClipTypesAsync(Dictionary<int, string> updates)
        {
            ArgumentNullException.ThrowIfNull(updates);

            SqliteConnection? connection = null;
            SqliteTransaction? transaction = null;
            try
            {
                connection = await GetOpenConnectionAsync().ConfigureAwait(false);
                transaction = (SqliteTransaction)await connection.BeginTransactionAsync().ConfigureAwait(false);

                foreach (var update in updates)
                {
                    SqliteCommand? command = null;
                    try
                    {
                        command = connection.CreateCommand();
                        command.Transaction = transaction;
                        command.CommandText = "UPDATE clips SET ClipType = @ClipType WHERE Id = @Id";
                        command.Parameters.AddWithValue("@Id", update.Key);
                        command.Parameters.AddWithValue("@ClipType", update.Value);
                        await command.ExecuteNonQueryAsync().ConfigureAwait(false);
                    }
                    finally
                    {
                        if (command != null) { await command.DisposeAsync().ConfigureAwait(false); }
                    }
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
        }
    }
}