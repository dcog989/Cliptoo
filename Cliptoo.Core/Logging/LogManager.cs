using System;
using System.Collections.Concurrent;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading;
using System.Threading.Tasks;

namespace Cliptoo.Core.Logging
{
    public static class LogManager
    {
        private static string? _logFilePath;
        public static string? LogFilePath => _logFilePath;
        private static string? _logFolder;
        private static DateTime _currentLogDate;
        private static readonly object _fileLock = new object();
        private static string? _appDataPath;

        private static readonly ConcurrentQueue<string> _logQueue = new();
        private static Task? _logWriterTask;
        private static CancellationTokenSource _cancellationTokenSource = new();

        public static LogLevel LoggingLevel { get; private set; } = LogLevel.Warning;
        public static bool IsInitialized { get; private set; }

        public static void Initialize(string appDataPath)
        {
            if (IsInitialized) return;

            _appDataPath = appDataPath;
            try
            {
                _logFolder = Path.Combine(appDataPath, "Cliptoo", "Logs");
                Directory.CreateDirectory(_logFolder);
                _logFilePath = Path.Combine(_logFolder, "cliptoo-latest.log");

                RotateLogFileIfNeeded();
                _currentLogDate = DateTime.Now.Date;

                IsInitialized = true;

                _logWriterTask = Task.Run(ProcessLogQueue, _cancellationTokenSource.Token);

                LogInfo($"--------------------------------------------------------------");
                LogInfo($"LogManager initialized successfully on {DateTime.Now:yyyyMMdd}.");
                LogInfo($"--------------------------------------------------------------");
            }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException or System.Security.SecurityException)
            {
                Console.WriteLine($"FATAL: Could not initialize primary LogManager: {ex.Message}");
                try
                {
                    _logFolder = Path.Combine(Path.GetTempPath(), "Cliptoo", "Logs");
                    Directory.CreateDirectory(_logFolder);
                    _logFilePath = Path.Combine(_logFolder, "cliptoo-fallback-latest.log");

                    RotateLogFileIfNeeded();
                    _currentLogDate = DateTime.Now.Date;

                    IsInitialized = true;
                    _logWriterTask = Task.Run(ProcessLogQueue, _cancellationTokenSource.Token);
                    LogWarning($"Using fallback log location due to error: {ex.Message}");
                }
                catch (Exception fallbackEx) when (fallbackEx is IOException or UnauthorizedAccessException or System.Security.SecurityException)
                {
                    IsInitialized = false;
                    var message = $"FATAL: Could not initialize fallback LogManager: {fallbackEx.Message}";
                    Console.WriteLine(message);
                    throw new InvalidOperationException(message, fallbackEx);
                }
            }
        }

        public static void Configure(LogLevel level, int retentionDays)
        {
            LoggingLevel = level;
            CleanUpOldLogs(retentionDays);
        }

        public static void Shutdown()
        {
            if (!IsInitialized) return;

            LogInfo("LogManager shutting down...");
            _cancellationTokenSource.Cancel();
            _logWriterTask?.Wait(TimeSpan.FromSeconds(2));
            IsInitialized = false;
        }

        private static async Task ProcessLogQueue()
        {
            try
            {
                while (!_cancellationTokenSource.Token.IsCancellationRequested)
                {
                    if (_logQueue.TryDequeue(out var message))
                    {
                        WriteToFile(message);
                    }
                    else
                    {
                        await Task.Delay(50, _cancellationTokenSource.Token).ConfigureAwait(false);
                    }
                }
            }
            catch (TaskCanceledException)
            {
                // Expected on shutdown
            }
            finally
            {
                // Process any remaining messages after cancellation is requested
                while (_logQueue.TryDequeue(out var message))
                {
                    WriteToFile(message);
                }
            }
        }

        private static void WriteToFile(string message)
        {
            if (string.IsNullOrEmpty(_logFilePath)) return;

            try
            {
                lock (_fileLock)
                {
                    if (DateTime.Now.Date > _currentLogDate)
                    {
                        RotateLogFileIfNeeded();
                        _currentLogDate = DateTime.Now.Date;
                        File.AppendAllText(_logFilePath, $"[{DateTime.Now:HH:mm:ss.fff}] INFO: --------------------------------------------------------------{Environment.NewLine}");
                        File.AppendAllText(_logFilePath, $"[{DateTime.Now:HH:mm:ss.fff}] INFO: LogManager initialized successfully on {DateTime.Now:yyyyMMdd}.{Environment.NewLine}");
                        File.AppendAllText(_logFilePath, $"[{DateTime.Now:HH:mm:ss.fff}] INFO: --------------------------------------------------------------{Environment.NewLine}");
                    }
                    File.AppendAllText(_logFilePath, message + Environment.NewLine);
                }
            }
            catch (IOException ex)
            {
                Console.WriteLine($"Failed to write to log: {ex.Message}");
            }
        }

        public static void ClearLogs()
        {
            if (string.IsNullOrEmpty(_logFolder) || _appDataPath is null) return;

            lock (_fileLock)
            {
                if (!IsInitialized) return;

                var wasInitialized = IsInitialized;
                IsInitialized = false; // Stop logging temporarily

                _cancellationTokenSource.Cancel();
                _logWriterTask?.Wait(TimeSpan.FromSeconds(1));

                try
                {
                    var directory = new DirectoryInfo(_logFolder);
                    foreach (var file in directory.EnumerateFiles("*.log*"))
                    {
                        try
                        {
                            file.Delete();
                        }
                        catch (IOException)
                        {
                            // File might be locked, just ignore.
                        }
                    }
                }
                catch (Exception ex) when (ex is IOException or UnauthorizedAccessException or DirectoryNotFoundException or System.Security.SecurityException)
                {
                    Console.WriteLine($"Failed to clear logs: {ex.Message}");
                }
                finally
                {
                    if (wasInitialized)
                    {
                        // Re-initialize
                        _cancellationTokenSource = new CancellationTokenSource();
                        Initialize(_appDataPath);
                        LogInfo("Log files cleared by user.");
                    }
                }
            }
        }

        private static void CleanUpOldLogs(int logRetentionDays)
        {
            if (string.IsNullOrEmpty(_logFolder) || logRetentionDays <= 0) return;
            try
            {
                var directory = new DirectoryInfo(_logFolder);
                var cutoff = DateTime.Now.Date.AddDays(-logRetentionDays);

                var oldLogs = directory.EnumerateFiles("cliptoo-*.log")
                    .Where(f =>
                    {
                        var name = Path.GetFileNameWithoutExtension(f.Name);
                        var parts = name.Split('-');
                        if (parts.Length < 2) return false;

                        if (DateTime.TryParseExact(parts[1], "yyMMdd", CultureInfo.InvariantCulture, DateTimeStyles.None, out var date))
                        {
                            return date < cutoff;
                        }
                        return false;
                    });

                foreach (var log in oldLogs)
                {
                    try
                    {
                        log.Delete();
                    }
                    catch (IOException)
                    {
                        // Ignore locked files
                    }
                }
            }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException or DirectoryNotFoundException or System.Security.SecurityException)
            {
                Console.WriteLine($"Failed to clean up old logs: {ex.Message}");
            }
        }


        private static void RotateLogFileIfNeeded()
        {
            if (string.IsNullOrEmpty(_logFilePath) || !File.Exists(_logFilePath))
            {
                return;
            }

            try
            {
                var fileInfo = new FileInfo(_logFilePath);
                if (fileInfo.LastWriteTime.Date < DateTime.Now.Date)
                {
                    var logDate = (_currentLogDate == DateTime.MinValue) ? fileInfo.LastWriteTime.Date : _currentLogDate;
                    var newName = Path.GetFileNameWithoutExtension(_logFilePath).Replace("-latest", "", StringComparison.Ordinal) + $"-{logDate:yyMMdd}-{fileInfo.LastWriteTime:HHmmss}.log";

                    var directoryName = fileInfo.DirectoryName;
                    if (directoryName is null) return;

                    var newPath = Path.Combine(directoryName, newName);
                    File.Move(_logFilePath, newPath);
                }
            }
            catch (IOException ex)
            {
                Console.WriteLine($"Failed to rotate log file: {ex.Message}");
            }
        }

        private static void EnqueueMessage(LogLevel level, string message)
        {
            if (!IsInitialized || level > LoggingLevel) return;

            var formattedMessage = $"[{DateTime.Now:HH:mm:ss.fff}] {level.ToString().ToUpperInvariant()}: {message}";
            _logQueue.Enqueue(formattedMessage);
        }

        public static void LogDebug(string message) => EnqueueMessage(LogLevel.Debug, message);
        public static void LogInfo(string message) => EnqueueMessage(LogLevel.Info, message);
        public static void LogWarning(string message) => EnqueueMessage(LogLevel.Warning, message);
        public static void LogError(string message) => EnqueueMessage(LogLevel.Error, message);
        public static void LogCritical(string message) => EnqueueMessage(LogLevel.Critical, message);

        public static void LogCritical(Exception exception, string? context = null)
        {
            if (!IsInitialized || LogLevel.Critical > LoggingLevel) return;

            ArgumentNullException.ThrowIfNull(exception);

            var sb = new StringBuilder();
            var initialMessage = string.IsNullOrEmpty(context)
                ? "An exception occurred."
                : $"An exception occurred: {context}";

            sb.AppendLine(initialMessage);
            sb.AppendLine(CultureInfo.InvariantCulture, $"Type: {exception.GetType().FullName}");
            sb.AppendLine(CultureInfo.InvariantCulture, $"Message: {exception.Message}");
            sb.AppendLine("StackTrace:");
            sb.AppendLine(exception.StackTrace);

            if (exception.InnerException != null)
            {
                sb.AppendLine("--- Inner Exception ---");
                sb.AppendLine(CultureInfo.InvariantCulture, $"Type: {exception.InnerException.GetType().FullName}");
                sb.AppendLine(CultureInfo.InvariantCulture, $"Message: {exception.InnerException.Message}");
                sb.AppendLine("StackTrace:");
                sb.AppendLine(exception.InnerException.StackTrace);
            }
            sb.AppendLine("--------------------------");

            EnqueueMessage(LogLevel.Critical, sb.ToString());
        }
    }
}