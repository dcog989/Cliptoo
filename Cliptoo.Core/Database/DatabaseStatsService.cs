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

        public Task UpdatePasteCountAsync()
        {
            return ExecuteNonQueryAsync("UPDATE stats SET Value = COALESCE(Value, 0) + 1 WHERE Key = 'PasteCount'");
        }

        public Task UpdateLastCleanupTimestampAsync()
        {
            var sql = "INSERT OR REPLACE INTO stats (Key, TextValue) VALUES ('LastCleanupTimestamp', @Timestamp);";
            var param = new SqliteParameter("@Timestamp", DateTime.UtcNow.ToString("o"));
            return ExecuteNonQueryAsync(sql, param);
        }

        public async Task<DbStats> GetStatsAsync()
        {
            long totalClips = 0;
            long pinnedClips = 0;
            long totalContentLength = 0;
            long pasteCount = 0;
            long uniqueClipsEver = 0;
            DateTime? creationTimestamp = null;
            DateTime? lastCleanupTimestamp = null;

            SqliteConnection? connection = null;
            try
            {
                connection = await GetOpenConnectionAsync().ConfigureAwait(false);
                SqliteCommand? command = null;
                SqliteDataReader? reader = null;
                try
                {
                    command = connection.CreateCommand();
                    command.CommandText = "SELECT COUNT(*), COALESCE(SUM(LENGTH(Content)), 0), SUM(CASE WHEN IsPinned = 1 THEN 1 ELSE 0 END) FROM clips";
                    reader = await command.ExecuteReaderAsync().ConfigureAwait(false);
                    if (await reader.ReadAsync().ConfigureAwait(false))
                    {
                        totalClips = reader.GetInt64(0);
                        totalContentLength = reader.GetInt64(1);
                        pinnedClips = reader.GetInt64(2);
                    }
                }
                finally
                {
                    if (reader != null) { await reader.DisposeAsync().ConfigureAwait(false); }
                    if (command != null) { await command.DisposeAsync().ConfigureAwait(false); }
                }

                try
                {
                    command = connection.CreateCommand();
                    command.CommandText = "SELECT Key, Value, TextValue FROM stats WHERE Key IN ('PasteCount', 'UniqueClipsEver', 'CreationTimestamp', 'LastCleanupTimestamp')";
                    reader = await command.ExecuteReaderAsync().ConfigureAwait(false);
                    while (await reader.ReadAsync().ConfigureAwait(false))
                    {
                        var key = reader.GetString(0);
                        switch (key)
                        {
                            case "PasteCount" when !await reader.IsDBNullAsync(1, CancellationToken.None).ConfigureAwait(false):
                                pasteCount = reader.GetInt64(1);
                                break;
                            case "UniqueClipsEver" when !await reader.IsDBNullAsync(1, CancellationToken.None).ConfigureAwait(false):
                                uniqueClipsEver = reader.GetInt64(1);
                                break;
                            case "CreationTimestamp" when !await reader.IsDBNullAsync(2, CancellationToken.None).ConfigureAwait(false):
                                if (DateTime.TryParse(reader.GetString(2), null, System.Globalization.DateTimeStyles.RoundtripKind, out var createDt))
                                {
                                    creationTimestamp = createDt.ToLocalTime();
                                }
                                break;
                            case "LastCleanupTimestamp" when !await reader.IsDBNullAsync(2, CancellationToken.None).ConfigureAwait(false):
                                if (DateTime.TryParse(reader.GetString(2), null, System.Globalization.DateTimeStyles.RoundtripKind, out var cleanupDt))
                                {
                                    lastCleanupTimestamp = cleanupDt.ToLocalTime();
                                }
                                break;
                        }
                    }
                }
                finally
                {
                    if (reader != null) { await reader.DisposeAsync().ConfigureAwait(false); }
                    if (command != null) { await command.DisposeAsync().ConfigureAwait(false); }
                }
            }
            finally
            {
                if (connection != null) { await connection.DisposeAsync().ConfigureAwait(false); }
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
                UniqueClipsEver = uniqueClipsEver,
                CreationTimestamp = creationTimestamp,
                PinnedClips = pinnedClips,
                LastCleanupTimestamp = lastCleanupTimestamp
            };
        }
    }
}