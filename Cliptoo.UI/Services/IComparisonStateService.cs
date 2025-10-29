namespace Cliptoo.UI.Services
{
    public class ComparisonStateChangedEventArgs : EventArgs
    {
        public int? OldLeftClipId { get; }
        public int? NewLeftClipId { get; }

        public ComparisonStateChangedEventArgs(int? oldLeftClipId, int? newLeftClipId)
        {
            OldLeftClipId = oldLeftClipId;
            NewLeftClipId = newLeftClipId;
        }
    }

    public interface IComparisonStateService
    {
        int? LeftClipId { get; }
        bool IsCompareToolAvailable { get; }

        void SelectLeftClip(int clipId);
        void ClearSelection();
        Task<(bool success, string message)> CompareWithRightClipAsync(int rightClipId);
        event EventHandler<ComparisonStateChangedEventArgs>? ComparisonStateChanged;
    }
}