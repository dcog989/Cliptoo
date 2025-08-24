namespace Cliptoo.Core.Services
{
    /// <summary>
    /// Provides syntax highlighting for code snippets.
    /// </summary>
    public interface ISyntaxHighlighter
    {
        /// <summary>
        /// Gets the name of the highlighting definition based on a clip's type.
        /// </summary>
        /// <param name="clipType">The type of the clip (e.g., "code_snippet", "file_dev").</param>
        /// <param name="content">The content of the clip, used for auto-detection.</param>
        /// <returns>The name of the syntax highlighting definition (e.g., "C#", "XML").</returns>
        string? GetHighlightingDefinition(string clipType, string content);
    }
}