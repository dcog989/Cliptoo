using System;
using System.Threading.Tasks;

namespace Cliptoo.Core.Database
{
    public class DatabaseInitializer : RepositoryBase, IDatabaseInitializer
    {
        private const int CurrentDbVersion = 9;

        public DatabaseInitializer(string dbPath) : base(dbPath) { }

        public async Task InitializeAsync()
        {
            await using var connection = await GetOpenConnectionAsync().ConfigureAwait(false);

            await using (var command = connection.CreateCommand())
            {
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
        }

        private async Task UpgradeDatabaseAsync(long fromVersion)
        {
            if (fromVersion >= CurrentDbVersion) return;

            await using var connection = await GetOpenConnectionAsync().ConfigureAwait(false);
            await using var transaction = (Microsoft.Data.Sqlite.SqliteTransaction)await connection.BeginTransactionAsync().ConfigureAwait(false);

            if (fromVersion < 8)
            {
                await using var alterCmd = connection.CreateCommand();
                alterCmd.Transaction = transaction;
                alterCmd.CommandText = "ALTER TABLE stats ADD COLUMN TextValue TEXT;";
                await alterCmd.ExecuteNonQueryAsync().ConfigureAwait(false);
            }

            if (fromVersion < 9)
            {
                await using var ftsCmd = connection.CreateCommand();
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

            await transaction.CommitAsync().ConfigureAwait(false);
        }
    }
}