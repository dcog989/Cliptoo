using System;

namespace Cliptoo.Core.Services.Models
{
    public class ProcessingFailedEventArgs : EventArgs
    {
        public string Title { get; }
        public string Message { get; }

        public ProcessingFailedEventArgs(string title, string message)
        {
            Title = title;
            Message = message;
        }
    }
}