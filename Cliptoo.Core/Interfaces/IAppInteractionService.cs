using System;

namespace Cliptoo.Core.Interfaces
{
    public interface IAppInteractionService
    {
        void NotifyUiActivity();
        bool IsUiInteractive { get; set; }
        DateTime LastActivityTimestamp { get; }
    }
}