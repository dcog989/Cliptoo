using System;
using System.Collections.Generic;
using System.IO;
using System.Threading.Tasks;
using Microsoft.Data.Sqlite;

namespace Cliptoo.Core.Database
{
    public class DatabaseMaintenanceService : RepositoryBase, IDatabaseMaintenanceService
    {
        private readonly string _dbPath;
        public DatabaseMaintenanceService(string dbPath) : base(dbPath)
        {
            _dbPath = dbPath;
        }

        public async Task<int> ClearHistoryAsync()
        {
            await using var connection = await GetOpenConnectionAsync();
            await using var command = connection.CreateCommand();
            command.CommandText = "DELETE FROM clips WHERE IsPinned = 0";
            var affected = await command.ExecuteNonQueryAsync();
            if (affected > 0) await CompactDbAsync();
            return affected;
        }

        public async Task<int> ClearAllHistoryAsync()
        {
            await using var connection = await GetOpenConnectionAsync();
            await using var command = connection.CreateCommand();
            command.CommandText = "DELETE FROM clips";
            var affected = await command.ExecuteNonQueryAsync();
            if (affected > 0) await CompactDbAsync();
            return affected;
        }

        public async Task CompactDbAsync()
        {
            Configuration.LogManager.LogDebug("DB_LOCK_DIAG: Starting database compaction.");

            // Connection pooling with WAL mode prevents VACUUM from getting an exclusive lock.
            // We must clear the pool to force all connections to the database file to close.
            var connectionForPoolClear = new SqliteConnection($"Data Source={_dbPath}");
            Configuration.LogManager.LogDebug("DB_LOCK_DIAG: Clearing SQLite connection pool...");
            SqliteConnection.ClearPool(connectionForPoolClear);
            Configuration.LogManager.LogDebug("DB_LOCK_DIAG: SQLite connection pool cleared.");
            await Task.Delay(100); // Give a moment for file locks to be released.

            var stopwatch = System.Diagnostics.Stopwatch.StartNew();
            await using var connection = await GetOpenConnectionAsync();
            await using var command = connection.CreateCommand();

            command.CommandText = "SELECT name FROM sqlite_master WHERE type='table' AND name='clips_fts';";
            var ftsTableExists = await command.ExecuteScalarAsync() != null;
            if (ftsTableExists)
            {
                Configuration.LogManager.LogDebug("DB_LOCK_DIAG: Optimizing FTS index...");
                var ftsStopwatch = System.Diagnostics.Stopwatch.StartNew();
                command.CommandText = "INSERT INTO clips_fts(clips_fts) VALUES('optimize');";
                await command.ExecuteNonQueryAsync();
                ftsStopwatch.Stop();
                Configuration.LogManager.LogDebug($"DB_LOCK_DIAG: FTS optimization finished in {ftsStopwatch.ElapsedMilliseconds}ms.");
            }

            Configuration.LogManager.LogDebug("DB_LOCK_DIAG: Starting VACUUM...");
            var vacuumStopwatch = System.Diagnostics.Stopwatch.StartNew();
            command.CommandText = "VACUUM;";
            await command.ExecuteNonQueryAsync();
            vacuumStopwatch.Stop();
            Configuration.LogManager.LogDebug($"DB_LOCK_DIAG: VACUUM finished in {vacuumStopwatch.ElapsedMilliseconds}ms.");
            stopwatch.Stop();
            Configuration.LogManager.LogDebug($"DB_LOCK_DIAG: Database compaction finished in {stopwatch.ElapsedMilliseconds}ms.");
        }

        public async Task<int> PerformCleanupAsync(uint days, uint maxClips, bool forceCompact = false)
        {
            Configuration.LogManager.LogDebug("DB_LOCK_DIAG: Starting cleanup process...");
            var stopwatch = System.Diagnostics.Stopwatch.StartNew();
            await using var connection = await GetOpenConnectionAsync();
            int totalAffected = 0;

            if (days > 0)
            {
                await using var ageCmd = connection.CreateCommand();
                ageCmd.CommandText = "DELETE FROM clips WHERE IsPinned = 0 AND Timestamp < @CutoffDate";
                ageCmd.Parameters.AddWithValue("@CutoffDate", DateTime.UtcNow.AddDays(-days).ToString("o"));
                totalAffected += await ageCmd.ExecuteNonQueryAsync();
            }

            if (maxClips > 0)
            {
                await using var countCmd = connection.CreateCommand();
                countCmd.CommandText = "SELECT COUNT(*) FROM clips WHERE IsPinned = 0";
                var count = (long)(await countCmd.ExecuteScalarAsync() ?? 0L);

                if (count > maxClips)
                {
                    await using var deleteCmd = connection.CreateCommand();
                    deleteCmd.CommandText = @"DELETE FROM clips WHERE Id IN (SELECT Id FROM clips WHERE IsPinned = 0 ORDER BY Timestamp ASC LIMIT @Limit)";
                    deleteCmd.Parameters.AddWithValue("@Limit", count - maxClips);
                    totalAffected += await deleteCmd.ExecuteNonQueryAsync();
                }
            }

            if (totalAffected > 0 || forceCompact)
            {
                await CompactDbAsync();
            }
            stopwatch.Stop();
            Configuration.LogManager.LogDebug($"DB_LOCK_DIAG: Cleanup process finished in {stopwatch.ElapsedMilliseconds}ms. Removed {totalAffected} clips.");

            return totalAffected;
        }

        public async Task<int> RemoveDeadheadClipsAsync()
        {
            await using var connection = await GetOpenConnectionAsync();
            var idsToDelete = new List<int>();
            const int batchSize = 500;
            var hasMoreRows = true;
            var offset = 0;
            int totalAffected = 0;

            while (hasMoreRows)
            {
                var clipsToCheck = new List<(int Id, string Content, string ClipType)>();
                await using (var command = connection.CreateCommand())
                {
                    command.CommandText = "SELECT Id, Content, ClipType FROM clips WHERE ClipType LIKE 'file_%' OR ClipType = 'folder' LIMIT @BatchSize OFFSET @Offset";
                    command.Parameters.AddWithValue("@BatchSize", batchSize);
                    command.Parameters.AddWithValue("@Offset", offset);

                    await using var reader = await command.ExecuteReaderAsync();
                    while (await reader.ReadAsync())
                    {
                        clipsToCheck.Add((reader.GetInt32(0), reader.GetString(1), reader.GetString(2)));
                    }
                }

                if (clipsToCheck.Count < batchSize)
                {
                    hasMoreRows = false;
                }

                offset += clipsToCheck.Count;

                await Task.Run(() =>
                {
                    foreach (var clip in clipsToCheck)
                    {
                        var contentPath = clip.Content.Trim();
                        bool shouldDelete = false;

                        if (clip.ClipType == AppConstants.ClipTypes.Folder)
                        {
                            if (!Directory.Exists(contentPath)) shouldDelete = true;
                        }
                        else if (clip.ClipType.StartsWith("file_"))
                        {
                            if (!File.Exists(contentPath)) shouldDelete = true;
                        }

                        if (shouldDelete)
                        {
                            lock (idsToDelete)
                            {
                                idsToDelete.Add(clip.Id);
                            }
                        }
                    }
                });

                if (idsToDelete.Count > 0)
                {
                    await using var transaction = (SqliteTransaction)await connection.BeginTransactionAsync();
                    foreach (var id in idsToDelete)
                    {
                        await using var deleteCmd = connection.CreateCommand();
                        deleteCmd.Transaction = transaction;
                        deleteCmd.CommandText = "DELETE FROM clips WHERE Id = @Id";
                        deleteCmd.Parameters.AddWithValue("@Id", id);
                        totalAffected += await deleteCmd.ExecuteNonQueryAsync();
                    }
                    await transaction.CommitAsync();
                    idsToDelete.Clear();
                }
            }

            if (totalAffected > 0)
            {
                await CompactDbAsync();
            }

            return totalAffected;
        }

        public async Task<int> ClearOversizedClipsAsync(uint sizeMb)
        {
            var sizeBytes = (long)sizeMb * 1024 * 1024;
            await using var connection = await GetOpenConnectionAsync();
            await using var command = connection.CreateCommand();

            command.CommandText = "DELETE FROM clips WHERE IsPinned = 0 AND SizeInBytes > @SizeBytes";
            command.Parameters.AddWithValue("@SizeBytes", sizeBytes);

            int totalAffected = await command.ExecuteNonQueryAsync();

            if (totalAffected > 0)
            {
                await CompactDbAsync();
            }

            return totalAffected;
        }

    }
}