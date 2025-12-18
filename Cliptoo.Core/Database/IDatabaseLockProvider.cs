using System;
using System.Threading;
using System.Threading.Tasks;

namespace Cliptoo.Core.Database
{
    public interface IDatabaseLockProvider
    {
        Task<IDisposable> AcquireLockAsync(CancellationToken cancellationToken = default);
        bool IsMaintenanceMode { get; set; }
    }

    public class DatabaseLockProvider : IDatabaseLockProvider
    {
        private readonly SemaphoreSlim _lock = new(1, 1);
        public bool IsMaintenanceMode { get; set; }

        public async Task<IDisposable> AcquireLockAsync(CancellationToken cancellationToken = default)
        {
            await _lock.WaitAsync(cancellationToken).ConfigureAwait(false);
            return new Releaser(_lock);
        }

        private sealed class Releaser : IDisposable
        {
            private readonly SemaphoreSlim _semaphore;
            private bool _disposed;

            public Releaser(SemaphoreSlim semaphore) => _semaphore = semaphore;

            public void Dispose()
            {
                if (!_disposed)
                {
                    _semaphore.Release();
                    _disposed = true;
                }
            }
        }
    }
}
