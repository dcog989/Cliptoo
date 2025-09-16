using System;

namespace Cliptoo.Core.Native.Models
{
    public enum ClipboardContentType
    {
        Text,
        Image,
        FileDrop
    }

    public class ClipboardChangedEventArgs : EventArgs
    {
        public object Content { get; }
        public ClipboardContentType ContentType { get; }
        public string? SourceApp { get; }
        public bool IsRtf { get; }

        public ClipboardChangedEventArgs(object content, ClipboardContentType contentType, string? sourceApp, bool isRtf = false)
        {
            Content = content;
            ContentType = contentType;
            SourceApp = sourceApp;
            IsRtf = isRtf;
        }
    }
}