using System;
using System.IO;
using System.Threading;
using System.Threading.Tasks;
using Cliptoo.Core.Database.Models;
using Microsoft.Data.Sqlite;

namespace Cliptoo.Core.Database
{
    public class DatabaseStatsService : RepositoryBase, IDatabaseStatsService
    {
        private readonly string _dbPath;
        public DatabaseStatsService(string dbPath) : base(dbPath)
        {
            _dbPath = dbPath;
        }

        public async Task UpdatePasteCountAsync()
        {
            SqliteConnection? connection = null;
            SqliteCommand? command = null;
            try
            {
                connection = await GetOpenConnectionAsync().ConfigureAwait(false);
                command = connection.CreateCommand();
                command.CommandText = "UPDATE stats SET Value = COALESCE(Value, 0) + 1 WHERE Key = 'PasteCount'";
                await command.ExecuteNonQueryAsync().ConfigureAwait(false);
            }
            finally
            {
                if (command != null) { await command.DisposeAsync().ConfigureAwait(false); }
                if (connection != null) { await connection.DisposeAsync().ConfigureAwait(false); }
            }
        }

        public async Task UpdateLastCleanupTimestampAsync()
        {
            SqliteConnection? connection = null;
            SqliteCommand? command = null;
            try
            {
                connection = await GetOpenConnectionAsync().ConfigureAwait(false);
                command = connection.CreateCommand();
                command.CommandText = "INSERT OR REPLACE INTO stats (Key, TextValue) VALUES ('LastCleanupTimestamp', @Timestamp);";
                command.Parameters.AddWithValue("@Timestamp", DateTime.UtcNow.ToString("o"));
                await command.ExecuteNonQueryAsync().ConfigureAwait(false);
            }
            finally
            {
                if (command != null) { await command.DisposeAsync().ConfigureAwait(false); }
                if (connection != null) { await connection.DisposeAsync().ConfigureAwait(false); }
            }
        }

        public async Task<DbStats> GetStatsAsync()
        {
            SqliteConnection? connection = null;
            try
            {
                connection = await GetOpenConnectionAsync().ConfigureAwait(false);
                long totalClips = 0;
                long pinnedClips = 0;
                long totalContentLength = 0;
                long pasteCount = 0;
                long totalClipsEver = 0;
                DateTime? creationTimestamp = null;
                DateTime? lastCleanupTimestamp = null;

                SqliteCommand? command1 = null;
                SqliteDataReader? reader1 = null;
                try
                {
                    command1 = connection.CreateCommand();
                    command1.CommandText = "SELECT COUNT(*), COALESCE(SUM(LENGTH(Content)), 0) FROM clips";
                    reader1 = await command1.ExecuteReaderAsync().ConfigureAwait(false);
                    if (await reader1.ReadAsync().ConfigureAwait(false))
                    {
                        totalClips = reader1.GetInt64(0);
                        totalContentLength = reader1.GetInt64(1);
                    }
                }
                finally
                {
                    if (reader1 != null) { await reader1.DisposeAsync().ConfigureAwait(false); }
                    if (command1 != null) { await command1.DisposeAsync().ConfigureAwait(false); }
                }

                SqliteCommand? command2 = null;
                SqliteDataReader? reader2 = null;
                try
                {
                    command2 = connection.CreateCommand();
                    command2.CommandText = "SELECT COUNT(*) FROM clips WHERE IsPinned = 1";
                    reader2 = await command2.ExecuteReaderAsync().ConfigureAwait(false);
                    if (await reader2.ReadAsync().ConfigureAwait(false))
                    {
                        pinnedClips = reader2.GetInt64(0);
                    }
                }
                finally
                {
                    if (reader2 != null) { await reader2.DisposeAsync().ConfigureAwait(false); }
                    if (command2 != null) { await command2.DisposeAsync().ConfigureAwait(false); }
                }

                SqliteCommand? command3 = null;
                SqliteDataReader? reader3 = null;
                try
                {
                    command3 = connection.CreateCommand();
                    command3.CommandText = "SELECT Key, Value, TextValue FROM stats WHERE Key IN ('PasteCount', 'TotalClipsEver', 'CreationTimestamp', 'LastCleanupTimestamp')";
                    reader3 = await command3.ExecuteReaderAsync().ConfigureAwait(false);
                    while (await reader3.ReadAsync().ConfigureAwait(false))
                    {
                        var key = reader3.GetString(0);
                        switch (key)
                        {
                            case "PasteCount" when !await reader3.IsDBNullAsync(1, CancellationToken.None).ConfigureAwait(false):
                                pasteCount = reader3.GetInt64(1);
                                break;
                            case "TotalClipsEver" when !await reader3.IsDBNullAsync(1, CancellationToken.None).ConfigureAwait(false):
                                totalClipsEver = reader3.GetInt64(1);
                                break;
                            case "CreationTimestamp" when !await reader3.IsDBNullAsync(2, CancellationToken.None).ConfigureAwait(false):
                                if (DateTime.TryParse(reader3.GetString(2), null, System.Globalization.DateTimeStyles.RoundtripKind, out var createDt))
                                {
                                    creationTimestamp = createDt.ToLocalTime();
                                }
                                break;
                            case "LastCleanupTimestamp" when !await reader3.IsDBNullAsync(2, CancellationToken.None).ConfigureAwait(false):
                                if (DateTime.TryParse(reader3.GetString(2), null, System.Globalization.DateTimeStyles.RoundtripKind, out var cleanupDt))
                                {
                                    lastCleanupTimestamp = cleanupDt.ToLocalTime();
                                }
                                break;
                        }
                    }
                }
                finally
                {
                    if (reader3 != null) { await reader3.DisposeAsync().ConfigureAwait(false); }
                    if (command3 != null) { await command3.DisposeAsync().ConfigureAwait(false); }
                }


                double dbSizeMb = 0;
                if (File.Exists(_dbPath))
                {
                    dbSizeMb = Math.Round(new FileInfo(_dbPath).Length / (1024.0 * 1024.0), 2);
                }

                return new DbStats
                {
                    TotalClips = totalClips,
                    TotalContentLength = totalContentLength,
                    PasteCount = pasteCount,
                    DatabaseSizeMb = dbSizeMb,
                    TotalClipsEver = totalClipsEver,
                    CreationTimestamp = creationTimestamp,
                    PinnedClips = pinnedClips,
                    LastCleanupTimestamp = lastCleanupTimestamp
                };
            }
            finally
            {
                if (connection != null) { await connection.DisposeAsync().ConfigureAwait(false); }
            }
        }
    }
}