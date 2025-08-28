using System;
using System.Diagnostics.CodeAnalysis;
using System.Globalization;
using System.Threading.Tasks;
using Microsoft.Data.Sqlite;

namespace Cliptoo.Core.Database
{
    public abstract class RepositoryBase
    {
        private readonly string _connectionString;

        protected RepositoryBase(string dbPath)
        {
            _connectionString = $"Data Source={dbPath}";
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

        [SuppressMessage("Microsoft.Security", "CA2100:Review SQL queries for security vulnerabilities", Justification = "This is a helper method. Callers are responsible for using parameterized queries.")]
        protected async Task<int> ExecuteNonQueryAsync(string commandText, params SqliteParameter[] parameters)
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

        protected async Task ExecuteTransactionAsync(Func<SqliteConnection, System.Data.Common.DbTransaction, Task> transactionWork)
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
}