using System;
using System.Globalization;
using System.IO;
using System.Text;

namespace Cliptoo.Core.Configuration
{
    public static class LogManager
    {
        private static string? _logFilePath;
        public static string? LogFilePath => _logFilePath;
        private static string? _logFolder;
        private static DateTime _currentLogDate;
        private readonly static object _lock = new object();
        private static string? _appDataPath;

        public static string LoggingLevel { get; set; } = "Info";
        public static bool IsInitialized { get; private set; }

        public static void Initialize(string appDataPath)
        {
            _appDataPath = appDataPath;
            try
            {
                _logFolder = Path.Combine(appDataPath, "Cliptoo", "Logs");
                Directory.CreateDirectory(_logFolder);
                _logFilePath = Path.Combine(_logFolder, "cliptoo-latest.log");

                RotateLogFileIfNeeded();
                _currentLogDate = DateTime.Now.Date;

                IsInitialized = true;
                Log($"--------------------------------------------------------------");
                Log($"LogManager initialized successfully on {DateTime.Now:yyyyMMdd}.");
                Log($"--------------------------------------------------------------");
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
                    var message = $"FATAL: Could not initialize fallback LogManager: {fallbackEx.Message}";
                    Console.WriteLine(message);
                    // Throwing here will prevent the application from starting without logging, which is critical.
                    throw new InvalidOperationException(message, fallbackEx);
                }
            }
        }

        public static void ClearLogs()
        {
            if (string.IsNullOrEmpty(_logFolder) || _appDataPath is null) return;

            lock (_lock)
            {
                try
                {
                    IsInitialized = false; // Stop logging temporarily

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
                    Initialize(_appDataPath);
                    Log("Log files cleared by user.");
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
                        Log($"--------------------------------------------------------------");
                        Log($"LogManager initialized successfully on {DateTime.Now:yyyyMMdd}.");
                        Log($"--------------------------------------------------------------");
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
                var initialMessage = string.IsNullOrEmpty(context)
                    ? "An exception occurred."
                    : $"An exception occurred: {context}";

                sb.AppendLine(CultureInfo.InvariantCulture, $"[{DateTime.Now:HH:mm:ss.fff}] ERROR: {initialMessage}");
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