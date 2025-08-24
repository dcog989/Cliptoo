using System;

namespace Cliptoo.Core.Services
{
    public interface IFileTypeClassifier
    {
        event Action? FileTypesChanged;
        /// <summary>
        /// Classifies a given file path into a content type string.
        /// </summary>
        /// <param name="filePath">The absolute path to the file or directory.</param>
        /// <returns>A string identifier for the content type (e.g., "file_image", "folder").</returns>
        string Classify(string filePath);
    }
}