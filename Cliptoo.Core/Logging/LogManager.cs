using System;

namespace Cliptoo.Core.Logging
{
    public static class LogManager
    {
        private static ILogManager? _logger;
        private static ILogManager Logger => _logger ?? throw new InvalidOperationException("LogManager has not been initialized. Call Initialize() first.");

        public static string? LogFilePath => Logger.LogFilePath;
        public static bool IsInitialized => _logger?.IsInitialized ?? false;
        public static LogLevel LoggingLevel => Logger.LoggingLevel;

        public static void Initialize(string appDataPath)
        {
            // This method is for early initialization before the DI container is built.
            // It will be replaced by the singleton instance later.
            if (_logger != null) return;
            _logger = new Logger();
            _logger.Initialize(appDataPath);
        }

        public static void Initialize(ILogManager logger)
        {
            // Replaces the initial logger with the singleton instance from the DI container.
            var oldLogger = _logger as IDisposable;
            _logger = logger;
            oldLogger?.Dispose(); // Dispose the temporary logger if it exists.
        }

        public static void Configure(LogLevel level, int retentionDays) => Logger.Configure(level, retentionDays);
        public static void Shutdown() => Logger.Shutdown();
        public static void ClearLogs() => Logger.ClearLogs();

        public static void LogDebug(string message) => Logger.LogDebug(message);
        public static void LogInfo(string message) => Logger.LogInfo(message);
        public static void LogWarning(string message) => Logger.LogWarning(message);
        public static void LogError(string message) => Logger.LogError(message);
        public static void LogCritical(string message) => Logger.LogCritical(message);
        public static void LogCritical(Exception exception, string? context = null) => Logger.LogCritical(exception, context);
    }
}