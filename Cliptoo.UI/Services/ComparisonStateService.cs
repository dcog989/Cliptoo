using Cliptoo.Core.Interfaces;

namespace Cliptoo.UI.Services
{
    public class ComparisonStateService : IComparisonStateService
    {
        private readonly IClipboardService _clipboardService;
        private int? _leftClipId;

        public int? LeftClipId => _leftClipId;
        public bool IsCompareToolAvailable => _clipboardService.IsCompareToolAvailable();
        public event EventHandler<ComparisonStateChangedEventArgs>? ComparisonStateChanged;

        public ComparisonStateService(IClipboardService clipboardService)
        {
            _clipboardService = clipboardService;
        }

        public void SelectLeftClip(int clipId)
        {
            var oldId = _leftClipId;
            _leftClipId = (_leftClipId == clipId) ? null : clipId;
            ComparisonStateChanged?.Invoke(this, new ComparisonStateChangedEventArgs(oldId, _leftClipId));
        }

        public void ClearSelection()
        {
            var oldId = _leftClipId;
            if (oldId.HasValue)
            {
                _leftClipId = null;
                ComparisonStateChanged?.Invoke(this, new ComparisonStateChangedEventArgs(oldId, null));
            }
        }

        public async Task<(bool success, string message)> CompareWithRightClipAsync(int rightClipId)
        {
            if (!_leftClipId.HasValue)
            {
                return (false, "No left clip selected for comparison.");
            }

            var result = await _clipboardService.CompareClipsAsync(_leftClipId.Value, rightClipId);
            ClearSelection();
            return result;
        }
    }
}