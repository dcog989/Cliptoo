namespace Cliptoo.Core.Logging
{
    public enum LogLevel
    {
        Debug = 0,    // For detailed diagnostic information.
        Info = 1,     // For general application flow messages.
        Warning = 2,  // For potential issues or non-critical problems.
        Error = 3,    // For runtime errors that do not crash the app.
        Critical = 4  // For exceptions and unrecoverable errors.
    }
}