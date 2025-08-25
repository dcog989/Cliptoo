using System;
using System.Runtime.InteropServices;
using System.Threading;
using System.Threading.Tasks;
using Cliptoo.Core.Configuration;

namespace Cliptoo.Core.Native
{
    public static class ClipboardUtils
    {
        private const int MaxRetries = 10;
        private const int DelayMs = 50;

        public static T? SafeGet<T>(Func<T> getDataFunc)
        {
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
                catch (Exception ex)
                {
                    LogManager.Log(ex, "An unexpected error occurred while getting clipboard data.");
                    return default;
                }
            }
            LogManager.Log($"Failed to get clipboard data after {MaxRetries} retries.");
            return default;
        }

        public static async Task<bool> SafeSet(Action setDataAction)
        {
            for (int i = 0; i < MaxRetries; i++)
            {
                try
                {
                    setDataAction();
                    return true; // Success
                }
                catch (COMException ex) when ((uint)ex.ErrorCode == 0x800401D0) // CLIPBRD_E_CANT_OPEN
                {
                    await Task.Delay(DelayMs);
                }
                catch (Exception ex)
                {
                    LogManager.Log(ex, "An unexpected error occurred while setting clipboard data.");
                    return false;
                }
            }
            LogManager.Log($"Failed to set clipboard data after {MaxRetries} retries.");
            return false;
        }
    }
}