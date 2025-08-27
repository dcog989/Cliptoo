using System;
using System.Diagnostics.CodeAnalysis;
using System.Threading.Tasks;
using Microsoft.Data.Sqlite;

namespace Cliptoo.Core.Database
{
    public class DatabaseInitializer : RepositoryBase, IDatabaseInitializer
    {
        private const int CurrentDbVersion = 9;

        public DatabaseInitializer(string dbPath) : base(dbPath) { }

        [SuppressMessage("Microsoft.Security", "CA2100:Review SQL queries for security vulnerabilities", Justification = "PRAGMA user_version is a hardcoded value, not user input.")]
        public async Task InitializeAsync()
        {
            SqliteConnection? connection = null;
            SqliteCommand? command = null;
            try
            {
                connection = await GetOpenConnectionAsync().ConfigureAwait(false);
                command = connection.CreateCommand();
                command.CommandText = "PRAGMA journal_mode=WAL;";
                await command.ExecuteNonQueryAsync().ConfigureAwait(false);

                command.CommandText = "PRAGMA user_version;";
                var currentVersion = (long)(await command.ExecuteScalarAsync().ConfigureAwait(false) ?? 0L);

                if (currentVersion == 0)
                {
                    command.CommandText = @"
                    CREATE TABLE clips (
                        Id INTEGER PRIMARY KEY AUTOINCREMENT,
                        Content TEXT NOT NULL,
                        ContentHash TEXT,
                        PreviewContent TEXT,
                        Timestamp TEXT NOT NULL,
                        ClipType TEXT NOT NULL,
                        SourceApp TEXT,
                        IsPinned INTEGER NOT NULL DEFAULT 0,
                        WasTrimmed INTEGER NOT NULL DEFAULT 0,
                        SizeInBytes INTEGER NOT NULL DEFAULT 0
                    );
                    CREATE INDEX idx_clips_timestamp ON clips(Timestamp);
                    CREATE INDEX idx_clips_content_hash ON clips(ContentHash);
                    CREATE TABLE stats (
                        Key TEXT PRIMARY KEY,
                        Value INTEGER,
                        TextValue TEXT
                    );
                    CREATE VIRTUAL TABLE clips_fts USING fts5(
                        Content,
                        content='clips',
                        content_rowid='Id',
                        tokenize='porter'
                    );

                    CREATE TRIGGER clips_ai AFTER INSERT ON clips BEGIN
                        INSERT INTO clips_fts(rowid, Content) VALUES (new.Id, new.Content);
                    END;

                    CREATE TRIGGER clips_ad AFTER DELETE ON clips BEGIN
                        INSERT INTO clips_fts(clips_fts, rowid, Content) VALUES ('delete', old.Id, old.Content);
                    END;

                    CREATE TRIGGER clips_au AFTER UPDATE ON clips BEGIN
                        INSERT INTO clips_fts(clips_fts, rowid, Content) VALUES ('delete', old.Id, old.Content);
                        INSERT INTO clips_fts(rowid, Content) VALUES (new.Id, new.Content);
                    END;
                    ";
                    await command.ExecuteNonQueryAsync().ConfigureAwait(false);
                }
                else if (currentVersion < CurrentDbVersion)
                {
                    await UpgradeDatabaseAsync(currentVersion).ConfigureAwait(false);
                }

                command.CommandText = "INSERT OR IGNORE INTO stats (Key, Value) VALUES ('PasteCount', 0);";
                await command.ExecuteNonQueryAsync().ConfigureAwait(false);

                command.CommandText = "INSERT OR IGNORE INTO stats (Key, Value) VALUES ('TotalClipsEver', (SELECT COUNT(*) FROM clips));";
                await command.ExecuteNonQueryAsync().ConfigureAwait(false);

                command.CommandText = "INSERT OR IGNORE INTO stats (Key, Value, TextValue) VALUES ('CreationTimestamp', 0, @Timestamp);";
                command.Parameters.Clear();
                command.Parameters.AddWithValue("@Timestamp", DateTime.UtcNow.ToString("o"));
                await command.ExecuteNonQueryAsync().ConfigureAwait(false);

                command.CommandText = "INSERT OR IGNORE INTO stats (Key, TextValue) VALUES ('LastCleanupTimestamp', @Timestamp);";
                command.Parameters.Clear();
                command.Parameters.AddWithValue("@Timestamp", DateTime.UtcNow.ToString("o"));
                await command.ExecuteNonQueryAsync().ConfigureAwait(false);

                command.CommandText = $"PRAGMA user_version = {CurrentDbVersion};";
                await command.ExecuteNonQueryAsync().ConfigureAwait(false);
            }
            finally
            {
                if (command != null) { await command.DisposeAsync().ConfigureAwait(false); }
                if (connection != null) { await connection.DisposeAsync().ConfigureAwait(false); }
            }
        }

        private async Task UpgradeDatabaseAsync(long fromVersion)
        {
            if (fromVersion >= CurrentDbVersion) return;

            SqliteConnection? connection = null;
            SqliteTransaction? transaction = null;
            try
            {
                connection = await GetOpenConnectionAsync().ConfigureAwait(false);
                transaction = (SqliteTransaction)await connection.BeginTransactionAsync().ConfigureAwait(false);

                if (fromVersion < 8)
                {
                    SqliteCommand? alterCmd = null;
                    try
                    {
                        alterCmd = connection.CreateCommand();
                        alterCmd.Transaction = transaction;
                        alterCmd.CommandText = "ALTER TABLE stats ADD COLUMN TextValue TEXT;";
                        await alterCmd.ExecuteNonQueryAsync().ConfigureAwait(false);
                    }
                    finally
                    {
                        if (alterCmd != null) { await alterCmd.DisposeAsync().ConfigureAwait(false); }
                    }
                }

                if (fromVersion < 9)
                {
                    SqliteCommand? ftsCmd = null;
                    try
                    {
                        ftsCmd = connection.CreateCommand();
                        ftsCmd.Transaction = transaction;
                        ftsCmd.CommandText = @"
                        CREATE VIRTUAL TABLE IF NOT EXISTS clips_fts USING fts5(
                            Content,
                            content='clips',
                            content_rowid='Id',
                            tokenize='porter'
                        );

                        -- Re-populate the FTS table to ensure it's in sync
                        DELETE FROM clips_fts;
                        INSERT INTO clips_fts(rowid, Content) SELECT Id, Content FROM clips;

                        CREATE TRIGGER IF NOT EXISTS clips_ai AFTER INSERT ON clips BEGIN
                            INSERT INTO clips_fts(rowid, Content) VALUES (new.Id, new.Content);
                        END;

                        CREATE TRIGGER IF NOT EXISTS clips_ad AFTER DELETE ON clips BEGIN
                            INSERT INTO clips_fts(clips_fts, rowid, Content) VALUES ('delete', old.Id, old.Content);
                        END;

                        CREATE TRIGGER IF NOT EXISTS clips_au AFTER UPDATE ON clips BEGIN
                            INSERT INTO clips_fts(clips_fts, rowid, Content) VALUES ('delete', old.Id, old.Content);
                            INSERT INTO clips_fts(rowid, Content) VALUES (new.Id, new.Content);
                        END;
                    ";
                        await ftsCmd.ExecuteNonQueryAsync().ConfigureAwait(false);
                    }
                    finally
                    {
                        if (ftsCmd != null) { await ftsCmd.DisposeAsync().ConfigureAwait(false); }
                    }
                }

                await transaction.CommitAsync().ConfigureAwait(false);
            }
            finally
            {
                if (transaction != null) { await transaction.DisposeAsync().ConfigureAwait(false); }
                if (connection != null) { await connection.DisposeAsync().ConfigureAwait(false); }
            }
        }
    }
}