using System;
using System.Collections.Generic;
using System.Diagnostics.CodeAnalysis;
using System.IO;
using System.Linq;
using System.Threading.Tasks;
using Microsoft.Data.Sqlite;
using Cliptoo.Core.Logging;

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
            var affected = await ExecuteNonQueryAsync("DELETE FROM clips WHERE IsFavorite = 0").ConfigureAwait(false);
            if (affected > 0) await CompactDbAsync().ConfigureAwait(false);
            return affected;
        }

        public async Task<int> ClearAllHistoryAsync()
        {
            var affected = await ExecuteNonQueryAsync("DELETE FROM clips").ConfigureAwait(false);
            if (affected > 0) await CompactDbAsync().ConfigureAwait(false);
            return affected;
        }

        public async Task<int> ClearFavoriteClipsAsync()
        {
            var affected = await ExecuteNonQueryAsync("DELETE FROM clips WHERE IsFavorite = 1").ConfigureAwait(false);
            if (affected > 0) await CompactDbAsync().ConfigureAwait(false);
            return affected;
        }

        public async Task CompactDbAsync()
        {
            LogManager.LogDebug("DB_LOCK_DIAG: Starting database compaction.");
            var stopwatch = System.Diagnostics.Stopwatch.StartNew();
            const int maxRetries = 4;
            const int initialDelayMs = 250;

            for (int i = 0; i < maxRetries; i++)
            {
                try
                {
                    using (var connectionForPoolClear = new SqliteConnection($"Data Source={_dbPath}"))
                    {
                        SqliteConnection.ClearPool(connectionForPoolClear);
                    }
                    LogManager.LogDebug($"DB_LOCK_DIAG: SQLite connection pool cleared (Attempt {i + 1}).");

                    SqliteConnection? connection = null;
                    SqliteCommand? command = null;
                    try
                    {
                        connection = await GetOpenConnectionAsync().ConfigureAwait(false);
                        command = connection.CreateCommand();

                        command.CommandText = "SELECT name FROM sqlite_master WHERE type='table' AND name='clips_fts';";
                        var ftsTableExists = await command.ExecuteScalarAsync().ConfigureAwait(false) != null;
                        if (ftsTableExists)
                        {
                            LogManager.LogDebug("DB_LOCK_DIAG: Optimizing FTS index...");
                            var ftsStopwatch = System.Diagnostics.Stopwatch.StartNew();
                            command.CommandText = "INSERT INTO clips_fts(clips_fts) VALUES('optimize');";
                            await command.ExecuteNonQueryAsync().ConfigureAwait(false);
                            ftsStopwatch.Stop();
                            LogManager.LogDebug($"DB_LOCK_DIAG: FTS optimization finished in {ftsStopwatch.ElapsedMilliseconds}ms.");
                        }

                        LogManager.LogDebug("DB_LOCK_DIAG: Starting VACUUM...");
                        var vacuumStopwatch = System.Diagnostics.Stopwatch.StartNew();
                        command.CommandText = "VACUUM;";
                        await command.ExecuteNonQueryAsync().ConfigureAwait(false);
                        vacuumStopwatch.Stop();
                        LogManager.LogDebug($"DB_LOCK_DIAG: VACUUM finished in {vacuumStopwatch.ElapsedMilliseconds}ms.");
                        stopwatch.Stop();
                        LogManager.LogDebug($"DB_LOCK_DIAG: Database compaction successful in {stopwatch.ElapsedMilliseconds}ms on attempt {i + 1}.");
                        return; // Success
                    }
                    finally
                    {
                        if (command != null) { await command.DisposeAsync().ConfigureAwait(false); }
                        if (connection != null) { await connection.DisposeAsync().ConfigureAwait(false); }
                    }
                }
                catch (SqliteException ex) when (ex.SqliteErrorCode == 5) // SQLITE_BUSY
                {
                    if (i == maxRetries - 1)
                    {
                        LogManager.LogCritical(ex, $"DB_LOCK_DIAG: Database compaction failed after {maxRetries} attempts. The database remained locked.");
                        break;
                    }

                    var delay = initialDelayMs * (int)Math.Pow(2, i);
                    LogManager.LogDebug($"DB_LOCK_DIAG: Database is busy. Retrying compaction in {delay}ms... (Attempt {i + 1})");
                    await Task.Delay(delay).ConfigureAwait(false);
                }
            }
        }

        public async Task<int> PerformCleanupAsync(uint days, uint maxClips, bool forceCompact = false)
        {
            LogManager.LogDebug("DB_LOCK_DIAG: Starting cleanup process...");
            var stopwatch = System.Diagnostics.Stopwatch.StartNew();
            SqliteConnection? connection = null;
            try
            {
                connection = await GetOpenConnectionAsync().ConfigureAwait(false);
                int totalAffected = 0;

                if (days > 0)
                {
                    SqliteCommand? ageCmd = null;
                    try
                    {
                        ageCmd = connection.CreateCommand();
                        ageCmd.CommandText = "DELETE FROM clips WHERE IsFavorite = 0 AND Timestamp < @CutoffDate";
                        ageCmd.Parameters.AddWithValue("@CutoffDate", DateTime.UtcNow.AddDays(-days).ToString("o"));
                        totalAffected += await ageCmd.ExecuteNonQueryAsync().ConfigureAwait(false);
                    }
                    finally
                    {
                        if (ageCmd != null) { await ageCmd.DisposeAsync().ConfigureAwait(false); }
                    }
                }

                if (maxClips > 0)
                {
                    long count;
                    SqliteCommand? countCmd = null;
                    try
                    {
                        countCmd = connection.CreateCommand();
                        countCmd.CommandText = "SELECT COUNT(*) FROM clips WHERE IsFavorite = 0";
                        count = (long)(await countCmd.ExecuteScalarAsync().ConfigureAwait(false) ?? 0L);
                    }
                    finally
                    {
                        if (countCmd != null) { await countCmd.DisposeAsync().ConfigureAwait(false); }
                    }

                    if (count > maxClips)
                    {
                        SqliteCommand? deleteCmd = null;
                        try
                        {
                            deleteCmd = connection.CreateCommand();
                            deleteCmd.CommandText = @"DELETE FROM clips WHERE Id IN (SELECT Id FROM clips WHERE IsFavorite = 0 ORDER BY Timestamp ASC LIMIT @Limit)";
                            deleteCmd.Parameters.AddWithValue("@Limit", count - maxClips);
                            totalAffected += await deleteCmd.ExecuteNonQueryAsync().ConfigureAwait(false);
                        }
                        finally
                        {
                            if (deleteCmd != null) { await deleteCmd.DisposeAsync().ConfigureAwait(false); }
                        }
                    }
                }

                if (totalAffected > 0 || forceCompact)
                {
                    await CompactDbAsync().ConfigureAwait(false);
                }
                stopwatch.Stop();
                LogManager.LogDebug($"DB_LOCK_DIAG: Cleanup process finished in {stopwatch.ElapsedMilliseconds}ms. Removed {totalAffected} clips.");

                return totalAffected;
            }
            finally
            {
                if (connection != null) { await connection.DisposeAsync().ConfigureAwait(false); }
            }
        }

        [SuppressMessage("Microsoft.Security", "CA2100:Review SQL queries for security vulnerabilities", Justification = "The dynamic part of the query is constructed from generated parameter names, not user input.")]
        public async Task<int> RemoveDeadheadClipsAsync()
        {
            var allIdsToDelete = new List<int>();
            var checkTasks = new List<Task>();
            SqliteConnection? connection = null;

            try
            {
                connection = await GetOpenConnectionAsync().ConfigureAwait(false);
                const int batchSize = 500;
                var hasMoreRows = true;
                var offset = 0;

                while (hasMoreRows)
                {
                    var clipsToCheck = new List<(int Id, string Content, string ClipType)>();
                    SqliteCommand? command = null;
                    SqliteDataReader? reader = null;
                    try
                    {
                        command = connection.CreateCommand();
                        command.CommandText = "SELECT Id, Content, ClipType FROM clips WHERE ClipType LIKE 'file_%' OR ClipType = 'folder' LIMIT @BatchSize OFFSET @Offset";
                        command.Parameters.AddWithValue("@BatchSize", batchSize);
                        command.Parameters.AddWithValue("@Offset", offset);
                        reader = await command.ExecuteReaderAsync().ConfigureAwait(false);
                        while (await reader.ReadAsync().ConfigureAwait(false))
                        {
                            clipsToCheck.Add((reader.GetInt32(0), reader.GetString(1), reader.GetString(2)));
                        }
                    }
                    finally
                    {
                        if (reader != null) { await reader.DisposeAsync().ConfigureAwait(false); }
                        if (command != null) { await command.DisposeAsync().ConfigureAwait(false); }
                    }

                    if (clipsToCheck.Count < batchSize)
                    {
                        hasMoreRows = false;
                    }
                    offset += clipsToCheck.Count;

                    var batchToCheck = clipsToCheck.ToList(); // Capture the current batch
                    checkTasks.Add(Task.Run(() =>
                    {
                        foreach (var clip in batchToCheck)
                        {
                            var contentPath = clip.Content.Trim();
                            bool shouldDelete = false;
                            if (clip.ClipType == AppConstants.ClipTypeFolder)
                            {
                                if (!Directory.Exists(contentPath)) shouldDelete = true;
                            }
                            else if (clip.ClipType.StartsWith("file_", StringComparison.Ordinal))
                            {
                                if (!File.Exists(contentPath)) shouldDelete = true;
                            }

                            if (shouldDelete)
                            {
                                lock (allIdsToDelete)
                                {
                                    allIdsToDelete.Add(clip.Id);
                                }
                            }
                        }
                    }));
                }
            }
            finally
            {
                if (connection != null) { await connection.DisposeAsync().ConfigureAwait(false); }
            }

            await Task.WhenAll(checkTasks).ConfigureAwait(false);

            if (allIdsToDelete.Count > 0)
            {
                await ExecuteTransactionAsync(async (connection, transaction) =>
                {
                    const int deleteBatchSize = 100;
                    for (int i = 0; i < allIdsToDelete.Count; i += deleteBatchSize)
                    {
                        var batch = allIdsToDelete.Skip(i).Take(deleteBatchSize).ToList();
                        if (batch.Count == 0) continue;
                        SqliteCommand? command = null;
                        try
                        {
                            command = connection.CreateCommand();
                            command.Transaction = (SqliteTransaction)transaction;
                            var paramNames = new List<string>();
                            for (int j = 0; j < batch.Count; j++)
                            {
                                var paramName = $"@id{j}";
                                paramNames.Add(paramName);
                                command.Parameters.Add(new SqliteParameter(paramName, batch[j]));
                            }
                            command.CommandText = $"DELETE FROM clips WHERE Id IN ({string.Join(",", paramNames)})";
                            await command.ExecuteNonQueryAsync().ConfigureAwait(false);
                        }
                        finally
                        {
                            if (command != null) { await command.DisposeAsync().ConfigureAwait(false); }
                        }
                    }
                }).ConfigureAwait(false);

                await CompactDbAsync().ConfigureAwait(false);
            }

            return allIdsToDelete.Count;
        }

        public async Task<int> ClearOversizedClipsAsync(uint sizeMb)
        {
            var sizeBytes = (long)sizeMb * 1024 * 1024;
            var sql = "DELETE FROM clips WHERE IsFavorite = 0 AND SizeInBytes > @SizeBytes";
            var param = new SqliteParameter("@SizeBytes", sizeBytes);
            int totalAffected = await ExecuteNonQueryAsync(sql, param).ConfigureAwait(false);

            if (totalAffected > 0)
            {
                await CompactDbAsync().ConfigureAwait(false);
            }

            return totalAffected;
        }
    }
}