using System.Diagnostics;
using Cliptoo.Core.Configuration;

namespace Cliptoo.UI.Helpers
{
    public static class DebugUtils
    {
        public static void LogMemoryUsage(string context)
        {
            var memory = Process.GetCurrentProcess().WorkingSet64 / (1024.0 * 1024.0);
            LogManager.LogDebug($"MEM_DIAG ({context}): {memory:F2} MB");
        }
    }
}