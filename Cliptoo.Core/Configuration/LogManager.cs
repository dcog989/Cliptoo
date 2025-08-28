using System;
using System.Globalization;
using System.IO;
using System.Text;

namespace Cliptoo.Core.Configuration
{
    public static class LogManager
    {
        private static string? _logFilePath;
        private static string? _logFolder;
        private static DateTime _currentLogDate;
        private static readonly object _lock = new object();

        public static string LoggingLevel { get; set; } = "Info";
        public static bool IsInitialized { get; private set; }

        public static void Initialize(string appDataPath)
        {
            try
            {
                _logFolder = Path.Combine(appDataPath, "Cliptoo", "Logs");
                Directory.CreateDirectory(_logFolder);
                _logFilePath = Path.Combine(_logFolder, "cliptoo-latest.log");

                RotateLogFileIfNeeded();
                _currentLogDate = DateTime.Now.Date;

                IsInitialized = true;
                Log($"LogManager initialized successfully. ({DateTime.Now:yyyyMMdd})");
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
                    Log($"WARNING: Using fallback log location due to error: {ex.Message}");
                }
                catch (Exception fallbackEx) when (fallbackEx is IOException or UnauthorizedAccessException or System.Security.SecurityException)
                {
                    IsInitialized = false;
                    Console.WriteLine($"FATAL: Could not initialize fallback LogManager: {fallbackEx.Message}");
                }
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
                    // If called from Initialize, _currentLogDate is MinValue. Use LastWriteTime's date as a fallback.
                    // If called from Write (during continuous operation), _currentLogDate has the correct date of the log being rotated.
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

        private static void Write(string level, string message)
        {
            if (!IsInitialized || string.IsNullOrEmpty(_logFilePath)) return;

            try
            {
                lock (_lock)
                {
                    if (DateTime.Now.Date > _currentLogDate)
                    {
                        RotateLogFileIfNeeded();
                        _currentLogDate = DateTime.Now.Date;

                        IsInitialized = true;
                        Log($"LogManager initialized successfully. ({DateTime.Now:yyyyMMdd})");
                    }

                    File.AppendAllText(_logFilePath, $"[{DateTime.Now:HH:mm:ss.fff}] {level}: {message}{Environment.NewLine}");
                }
            }
            catch (IOException ex)
            {
                Console.WriteLine($"Failed to write to log: {ex.Message}");
            }
        }

        public static void Log(string message)
        {
            if (LoggingLevel == "None") return;
            Write("INFO", message);
        }

        public static void LogDebug(string message)
        {
            if (LoggingLevel != "Debug") return;
            Write("DEBUG", message);
        }

        public static void Log(Exception exception, string? context = null)
        {
            ArgumentNullException.ThrowIfNull(exception);

            if (!IsInitialized || string.IsNullOrEmpty(_logFilePath)) return;

            try
            {
                var sb = new StringBuilder();
                sb.AppendLine(CultureInfo.InvariantCulture, $"[{DateTime.Now:HH:mm:ss.fff}] ERROR: An exception occurred.");
                if (!string.IsNullOrEmpty(context))
                {
                    sb.AppendLine(CultureInfo.InvariantCulture, $"Context: {context}");
                }
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

                lock (_lock)
                {
                    File.AppendAllText(_logFilePath, sb.ToString());
                }
            }
            catch (IOException ex)
            {
                Console.WriteLine($"Failed to write exception to log: {ex.Message}");
            }
        }
    }
}