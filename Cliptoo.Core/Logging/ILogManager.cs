using System;

namespace Cliptoo.Core.Logging
{
    public interface ILogger
    {
        string? LogFilePath { get; }
        LogLevel LoggingLevel { get; }
        bool IsInitialized { get; }

        void Initialize(string appDataPath);
        void Configure(LogLevel level, int retentionDays);
        void Shutdown();
        void ClearLogs();
        void LogDebug(string message);
        void LogInfo(string message);
        void LogWarning(string message);
        void LogError(string message);
        void LogCritical(string message);
        void LogCritical(Exception exception, string? context = null);
    }
}