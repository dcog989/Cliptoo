using System;
using Cliptoo.Core.Interfaces;

namespace Cliptoo.Core.Services
{
    public class AppInteractionService : IAppInteractionService
    {
        public DateTime LastActivityTimestamp { get; private set; } = DateTime.UtcNow;
        public bool IsUiInteractive { get; set; }

        public void NotifyUiActivity()
        {
            LastActivityTimestamp = DateTime.UtcNow;
        }
    }
}