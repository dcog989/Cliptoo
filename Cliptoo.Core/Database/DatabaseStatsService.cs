using System;
using System.IO;
using System.Threading.Tasks;
using Cliptoo.Core.Database.Models;

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
            await using var connection = await GetOpenConnectionAsync();
            await using var command = connection.CreateCommand();
            command.CommandText = "UPDATE stats SET Value = COALESCE(Value, 0) + 1 WHERE Key = 'PasteCount'";
            await command.ExecuteNonQueryAsync();
        }

        public async Task UpdateLastCleanupTimestampAsync()
        {
            await using var connection = await GetOpenConnectionAsync();
            await using var command = connection.CreateCommand();
            command.CommandText = "INSERT OR REPLACE INTO stats (Key, TextValue) VALUES ('LastCleanupTimestamp', @Timestamp);";
            command.Parameters.AddWithValue("@Timestamp", DateTime.UtcNow.ToString("o"));
            await command.ExecuteNonQueryAsync();
        }

        public async Task<DbStats> GetStatsAsync()
        {
            await using var connection = await GetOpenConnectionAsync();
            long totalClips = 0;
            long pinnedClips = 0;
            long totalContentLength = 0;
            long pasteCount = 0;
            long totalClipsEver = 0;
            DateTime? creationTimestamp = null;
            DateTime? lastCleanupTimestamp = null;

            await using (var command = connection.CreateCommand())
            {
                command.CommandText = "SELECT COUNT(*), COALESCE(SUM(LENGTH(Content)), 0) FROM clips";
                await using var reader = await command.ExecuteReaderAsync();
                if (await reader.ReadAsync())
                {
                    totalClips = reader.GetInt64(0);
                    totalContentLength = reader.GetInt64(1);
                }
            }

            await using (var command = connection.CreateCommand())
            {
                command.CommandText = "SELECT COUNT(*) FROM clips WHERE IsPinned = 1";
                await using var reader = await command.ExecuteReaderAsync();
                if (await reader.ReadAsync())
                {
                    pinnedClips = reader.GetInt64(0);
                }
            }

            await using (var command = connection.CreateCommand())
            {
                command.CommandText = "SELECT Key, Value, TextValue FROM stats WHERE Key IN ('PasteCount', 'TotalClipsEver', 'CreationTimestamp', 'LastCleanupTimestamp')";
                await using var reader = await command.ExecuteReaderAsync();
                while (await reader.ReadAsync())
                {
                    var key = reader.GetString(0);
                    switch (key)
                    {
                        case "PasteCount" when !reader.IsDBNull(1):
                            pasteCount = reader.GetInt64(1);
                            break;
                        case "TotalClipsEver" when !reader.IsDBNull(1):
                            totalClipsEver = reader.GetInt64(1);
                            break;
                        case "CreationTimestamp" when !reader.IsDBNull(2):
                            if (DateTime.TryParse(reader.GetString(2), null, System.Globalization.DateTimeStyles.RoundtripKind, out var createDt))
                            {
                                creationTimestamp = createDt.ToLocalTime();
                            }
                            break;
                        case "LastCleanupTimestamp" when !reader.IsDBNull(2):
                            if (DateTime.TryParse(reader.GetString(2), null, System.Globalization.DateTimeStyles.RoundtripKind, out var cleanupDt))
                            {
                                lastCleanupTimestamp = cleanupDt.ToLocalTime();
                            }
                            break;
                    }
                }
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
    }
}