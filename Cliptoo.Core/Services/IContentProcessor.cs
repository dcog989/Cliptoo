using Cliptoo.Core.Services.Models;

namespace Cliptoo.Core.Services
{
    /// <summary>
    /// Analyzes raw clipboard content to determine its specific type.
    /// </summary>
    public interface IContentProcessor
    {
        /// <summary>
        /// Processes a string to determine its clip type.
        /// </summary>
        /// <param name="content">The raw text content from the clipboard.</param>
        /// <returns>A ProcessingResult containing the classified type and original content.</returns>
        ProcessingResult Process(string content);
    }
}