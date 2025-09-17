namespace Cliptoo.Core.Logging
{
    public enum LogLevel
    {
        Critical, // For exceptions and unrecoverable errors.
        Error,    // For runtime errors that do not crash the app.
        Warning,  // For potential issues or non-critical problems.
        Info,     // For general application flow messages.
        Debug     // For detailed diagnostic information.
    }
}