using System;
using System.Collections.Generic;
using System.Linq;
using Cliptoo.Core.Logging;

namespace Cliptoo.Core.Services
{
    public class TextTransformer : ITextTransformer
    {
        private readonly Dictionary<string, ITextTransformation> _transformations;

        public TextTransformer(IEnumerable<ITextTransformation> transformations)
        {
            _transformations = transformations.ToDictionary(t => t.TransformationType);
        }

        public string Transform(string content, string transformType)
        {
            ArgumentNullException.ThrowIfNull(content);
            ArgumentException.ThrowIfNullOrWhiteSpace(transformType);

            if (!_transformations.TryGetValue(transformType, out var transformation))
            {
                return content;
            }

            const int maxLogLength = 100;
            if (LogManager.LoggingLevel == LogLevel.Debug)
            {
                var truncatedInput = content.Length > maxLogLength
                    ? string.Concat(content.AsSpan(0, maxLogLength), "...")
                    : content;
                LogManager.LogDebug($"TRANSFORM_DIAG: Applying '{transformType}'. Input: '{truncatedInput}'.");
            }

            var result = transformation.Transform(content);

            if (LogManager.LoggingLevel == LogLevel.Debug)
            {
                var truncatedResult = result.Length > maxLogLength
                    ? string.Concat(result.AsSpan(0, maxLogLength), "...")
                    : result;
                LogManager.LogDebug($"TRANSFORM_DIAG: Result: '{truncatedResult}'.");
            }

            return result;
        }
    }
}
