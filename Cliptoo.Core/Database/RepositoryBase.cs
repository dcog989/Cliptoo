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
            await connection.OpenAsync();
            return connection;
        }
    }
}