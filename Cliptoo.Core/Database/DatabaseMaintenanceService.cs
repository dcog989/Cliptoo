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
            SqliteConnection? connection = null;
            SqliteCommand? command = null;
            try
            {
                connection = await GetOpenConnectionAsync().ConfigureAwait(false);
                command = connection.CreateCommand();
                command.CommandText = "DELETE FROM clips WHERE IsPinned = 0";
                var affected = await command.ExecuteNonQueryAsync().ConfigureAwait(false);
                if (affected > 0) await CompactDbAsync().ConfigureAwait(false);
                return affected;
            }
            finally
            {
                if (command != null) { await command.DisposeAsync().ConfigureAwait(false); }
                if (connection != null) { await connection.DisposeAsync().ConfigureAwait(false); }
            }
        }

        public async Task<int> ClearAllHistoryAsync()
        {
            SqliteConnection? connection = null;
            SqliteCommand? command = null;
            try
            {
                connection = await GetOpenConnectionAsync().ConfigureAwait(false);
                command = connection.CreateCommand();
                command.CommandText = "DELETE FROM clips";
                var affected = await command.ExecuteNonQueryAsync().ConfigureAwait(false);
                if (affected > 0) await CompactDbAsync().ConfigureAwait(false);
                return affected;
            }
            finally
            {
                if (command != null) { await command.DisposeAsync().ConfigureAwait(false); }
                if (connection != null) { await connection.DisposeAsync().ConfigureAwait(false); }
            }
        }

        public async Task CompactDbAsync()
        {
            Configuration.LogManager.LogDebug("DB_LOCK_DIAG: Starting database compaction.");
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
                    Configuration.LogManager.LogDebug($"DB_LOCK_DIAG: SQLite connection pool cleared (Attempt {i + 1}).");

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
                            Configuration.LogManager.LogDebug("DB_LOCK_DIAG: Optimizing FTS index...");
                            var ftsStopwatch = System.Diagnostics.Stopwatch.StartNew();
                            command.CommandText = "INSERT INTO clips_fts(clips_fts) VALUES('optimize');";
                            await command.ExecuteNonQueryAsync().ConfigureAwait(false);
                            ftsStopwatch.Stop();
                            Configuration.LogManager.LogDebug($"DB_LOCK_DIAG: FTS optimization finished in {ftsStopwatch.ElapsedMilliseconds}ms.");
                        }

                        Configuration.LogManager.LogDebug("DB_LOCK_DIAG: Starting VACUUM...");
                        var vacuumStopwatch = System.Diagnostics.Stopwatch.StartNew();
                        command.CommandText = "VACUUM;";
                        await command.ExecuteNonQueryAsync().ConfigureAwait(false);
                        vacuumStopwatch.Stop();
                        Configuration.LogManager.LogDebug($"DB_LOCK_DIAG: VACUUM finished in {vacuumStopwatch.ElapsedMilliseconds}ms.");
                        stopwatch.Stop();
                        Configuration.LogManager.LogDebug($"DB_LOCK_DIAG: Database compaction successful in {stopwatch.ElapsedMilliseconds}ms on attempt {i + 1}.");
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
                        Configuration.LogManager.Log(ex, $"DB_LOCK_DIAG: Database compaction failed after {maxRetries} attempts. The database remained locked.");
                        break;
                    }

                    var delay = initialDelayMs * (int)Math.Pow(2, i);
                    Configuration.LogManager.LogDebug($"DB_LOCK_DIAG: Database is busy. Retrying compaction in {delay}ms... (Attempt {i + 1})");
                    await Task.Delay(delay).ConfigureAwait(false);
                }
            }
        }

        public async Task<int> PerformCleanupAsync(uint days, uint maxClips, bool forceCompact = false)
        {
            Configuration.LogManager.LogDebug("DB_LOCK_DIAG: Starting cleanup process...");
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
                        ageCmd.CommandText = "DELETE FROM clips WHERE IsPinned = 0 AND Timestamp < @CutoffDate";
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
                        countCmd.CommandText = "SELECT COUNT(*) FROM clips WHERE IsPinned = 0";
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
                            deleteCmd.CommandText = @"DELETE FROM clips WHERE Id IN (SELECT Id FROM clips WHERE IsPinned = 0 ORDER BY Timestamp ASC LIMIT @Limit)";
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
                Configuration.LogManager.LogDebug($"DB_LOCK_DIAG: Cleanup process finished in {stopwatch.ElapsedMilliseconds}ms. Removed {totalAffected} clips.");

                return totalAffected;
            }
            finally
            {
                if (connection != null) { await connection.DisposeAsync().ConfigureAwait(false); }
            }
        }

        public async Task<int> RemoveDeadheadClipsAsync()
        {
            SqliteConnection? connection = null;
            try
            {
                connection = await GetOpenConnectionAsync().ConfigureAwait(false);
                var idsToDelete = new List<int>();
                const int batchSize = 500;
                var hasMoreRows = true;
                var offset = 0;
                int totalAffected = 0;

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
                            else if (clip.ClipType.StartsWith("file_", StringComparison.Ordinal))
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
                    }).ConfigureAwait(false);

                    if (idsToDelete.Count > 0)
                    {
                        SqliteTransaction? transaction = null;
                        try
                        {
                            transaction = (SqliteTransaction)await connection.BeginTransactionAsync().ConfigureAwait(false);
                            foreach (var id in idsToDelete)
                            {
                                SqliteCommand? deleteCmd = null;
                                try
                                {
                                    deleteCmd = connection.CreateCommand();
                                    deleteCmd.Transaction = transaction;
                                    deleteCmd.CommandText = "DELETE FROM clips WHERE Id = @Id";
                                    deleteCmd.Parameters.AddWithValue("@Id", id);
                                    totalAffected += await deleteCmd.ExecuteNonQueryAsync().ConfigureAwait(false);
                                }
                                finally
                                {
                                    if (deleteCmd != null) { await deleteCmd.DisposeAsync().ConfigureAwait(false); }
                                }
                            }
                            await transaction.CommitAsync().ConfigureAwait(false);
                        }
                        finally
                        {
                            if (transaction != null) { await transaction.DisposeAsync().ConfigureAwait(false); }
                        }
                        idsToDelete.Clear();
                    }
                }

                if (totalAffected > 0)
                {
                    await CompactDbAsync().ConfigureAwait(false);
                }

                return totalAffected;
            }
            finally
            {
                if (connection != null) { await connection.DisposeAsync().ConfigureAwait(false); }
            }
        }

        public async Task<int> ClearOversizedClipsAsync(uint sizeMb)
        {
            var sizeBytes = (long)sizeMb * 1024 * 1024;
            SqliteConnection? connection = null;
            SqliteCommand? command = null;
            try
            {
                connection = await GetOpenConnectionAsync().ConfigureAwait(false);
                command = connection.CreateCommand();

                command.CommandText = "DELETE FROM clips WHERE IsPinned = 0 AND SizeInBytes > @SizeBytes";
                command.Parameters.AddWithValue("@SizeBytes", sizeBytes);

                int totalAffected = await command.ExecuteNonQueryAsync().ConfigureAwait(false);

                if (totalAffected > 0)
                {
                    await CompactDbAsync().ConfigureAwait(false);
                }

                return totalAffected;
            }
            finally
            {
                if (command != null) { await command.DisposeAsync().ConfigureAwait(false); }
                if (connection != null) { await connection.DisposeAsync().ConfigureAwait(false); }
            }
        }
    }
}