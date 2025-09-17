using System;
using System.Runtime.InteropServices;
using System.Threading;
using System.Threading.Tasks;
using Cliptoo.Core.Logging;

namespace Cliptoo.Core.Native
{
    public static class ClipboardUtils
    {
        private const int MaxRetries = 10;
        private const int DelayMs = 50;

        public static T? SafeGet<T>(Func<T> getDataFunc)
        {
            ArgumentNullException.ThrowIfNull(getDataFunc);

            for (int i = 0; i < MaxRetries; i++)
            {
                try
                {
                    return getDataFunc();
                }
                catch (COMException ex) when ((uint)ex.ErrorCode == 0x800401D0) // CLIPBRD_E_CANT_OPEN
                {
                    Thread.Sleep(DelayMs);
                }
                catch (Exception ex) when (ex is NullReferenceException or InvalidOperationException)
                {
                    LogManager.LogCritical(ex, "An unexpected error occurred while getting clipboard data.");
                    return default;
                }
            }
            LogManager.LogWarning($"Failed to get clipboard data after {MaxRetries} retries.");
            return default;
        }

        public static async Task<bool> SafeSet(Action setDataAction)
        {
            ArgumentNullException.ThrowIfNull(setDataAction);

            for (int i = 0; i < MaxRetries; i++)
            {
                try
                {
                    setDataAction();
                    return true; // Success
                }
                catch (COMException ex) when ((uint)ex.ErrorCode == 0x800401D0) // CLIPBRD_E_CANT_OPEN
                {
                    await Task.Delay(DelayMs).ConfigureAwait(false);
                }
                catch (Exception ex) when (ex is NullReferenceException or InvalidOperationException)
                {
                    LogManager.LogCritical(ex, "An unexpected error occurred while setting clipboard data.");
                    return false;
                }
            }
            LogManager.LogWarning($"Failed to set clipboard data after {MaxRetries} retries.");
            return false;
        }

    }
}