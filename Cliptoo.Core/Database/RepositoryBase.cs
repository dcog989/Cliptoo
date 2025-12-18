using System;
using System.Collections.Generic;
using System.Diagnostics.CodeAnalysis;
using System.Globalization;
using System.Runtime.CompilerServices;
using System.Threading;
using System.Threading.Tasks;
using Microsoft.Data.Sqlite;

namespace Cliptoo.Core.Database
{
    public abstract class RepositoryBase
    {
        private readonly string _connectionString;
        private readonly IDatabaseLockProvider _lockProvider;

        protected RepositoryBase(string dbPath, IDatabaseLockProvider lockProvider)
        {
            _connectionString = $"Data Source={dbPath}";
            _lockProvider = lockProvider;
        }

        protected async Task<SqliteConnection> GetOpenConnectionAsync()
        {
            var connection = new SqliteConnection(_connectionString);
            await connection.OpenAsync().ConfigureAwait(false);
            return connection;
        }

        [SuppressMessage("Microsoft.Security", "CA2100:Review SQL queries for security vulnerabilities", Justification = "This is a helper method. Callers are responsible for using parameterized queries.")]
        protected async Task<T?> ExecuteScalarAsync<T>(string commandText, params SqliteParameter[] parameters)
        {
            using (await _lockProvider.AcquireLockAsync().ConfigureAwait(false))
            {
                SqliteConnection? connection = null;
                SqliteCommand? command = null;
                try
                {
                    connection = await GetOpenConnectionAsync().ConfigureAwait(false);
                    command = connection.CreateCommand();
                    command.CommandText = commandText;
                    command.Parameters.AddRange(parameters);
                    var result = await command.ExecuteScalarAsync().ConfigureAwait(false);
                    if (result == null || result is DBNull)
                    {
                        return default;
                    }
                    return (T)Convert.ChangeType(result, typeof(T), CultureInfo.InvariantCulture);
                }
                finally
                {
                    if (command != null) { await command.DisposeAsync().ConfigureAwait(false); }
                    if (connection != null) { await connection.DisposeAsync().ConfigureAwait(false); }
                }
            }
        }

        [SuppressMessage("Microsoft.Security", "CA2100:Review SQL queries for security vulnerabilities", Justification = "This is a helper method. Callers are responsible for using parameterized queries.")]
        protected async Task<int> ExecuteNonQueryAsync(string commandText, params SqliteParameter[] parameters)
        {
            using (await _lockProvider.AcquireLockAsync().ConfigureAwait(false))
            {
                SqliteConnection? connection = null;
                SqliteCommand? command = null;
                try
                {
                    connection = await GetOpenConnectionAsync().ConfigureAwait(false);
                    command = connection.CreateCommand();
                    command.CommandText = commandText;
                    command.Parameters.AddRange(parameters);
                    return await command.ExecuteNonQueryAsync().ConfigureAwait(false);
                }
                finally
                {
                    if (command != null) { await command.DisposeAsync().ConfigureAwait(false); }
                    if (connection != null) { await connection.DisposeAsync().ConfigureAwait(false); }
                }
            }
        }

        protected async Task ExecuteTransactionAsync(Func<SqliteConnection, System.Data.Common.DbTransaction, Task> transactionWork)
        {
            using (await _lockProvider.AcquireLockAsync().ConfigureAwait(false))
            {
                ArgumentNullException.ThrowIfNull(transactionWork);
                SqliteConnection? connection = null;
                System.Data.Common.DbTransaction? transaction = null;
                try
                {
                    connection = await GetOpenConnectionAsync().ConfigureAwait(false);
                    transaction = await connection.BeginTransactionAsync().ConfigureAwait(false);
                    await transactionWork(connection, transaction).ConfigureAwait(false);
                    await transaction.CommitAsync().ConfigureAwait(false);
                }
                finally
                {
                    if (transaction != null) { await transaction.DisposeAsync().ConfigureAwait(false); }
                    if (connection != null) { await connection.DisposeAsync().ConfigureAwait(false); }
                }
            }
        }

        [SuppressMessage("Microsoft.Security", "CA2100:Review SQL queries for security vulnerabilities", Justification = "This is a helper method. Callers are responsible for using parameterized queries.")]
        protected async Task<T?> QuerySingleOrDefaultAsync<T>(
            string commandText,
            Func<SqliteDataReader, T> map,
            CancellationToken cancellationToken = default,
            params SqliteParameter[] parameters) where T : class
        {
            using (await _lockProvider.AcquireLockAsync(cancellationToken).ConfigureAwait(false))
            {
                ArgumentNullException.ThrowIfNull(map);
                SqliteConnection? connection = null;
                SqliteCommand? command = null;
                SqliteDataReader? reader = null;
                try
                {
                    connection = await GetOpenConnectionAsync().ConfigureAwait(false);
                    command = connection.CreateCommand();
                    command.CommandText = commandText;
                    command.Parameters.AddRange(parameters);
                    reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false);
                    if (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
                    {
                        return map(reader);
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
        }

        [SuppressMessage("Microsoft.Security", "CA2100:Review SQL queries for security vulnerabilities", Justification = "This is a helper method. Callers are responsible for using parameterized queries.")]
        protected async IAsyncEnumerable<T> QueryAsync<T>(
            string commandText,
            Func<SqliteDataReader, T> map,
            [EnumeratorCancellation] CancellationToken cancellationToken = default,
            params SqliteParameter[] parameters)
        {
            using (await _lockProvider.AcquireLockAsync(cancellationToken).ConfigureAwait(false))
            {
                ArgumentNullException.ThrowIfNull(map);
                SqliteConnection? connection = null;
                SqliteCommand? command = null;
                SqliteDataReader? reader = null;
                try
                {
                    connection = await GetOpenConnectionAsync().ConfigureAwait(false);
                    command = connection.CreateCommand();
                    command.CommandText = commandText;
                    command.Parameters.AddRange(parameters);
                    reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false);
                    while (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
                    {
                        yield return map(reader);
                    }
                }
                finally
                {
                    if (reader != null) { await reader.DisposeAsync().ConfigureAwait(false); }
                    if (command != null) { await command.DisposeAsync().ConfigureAwait(false); }
                    if (connection != null) { await connection.DisposeAsync().ConfigureAwait(false); }
                }
            }
        }
    }
}
