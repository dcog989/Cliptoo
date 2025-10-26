using System.Windows;
using Cliptoo.UI.ViewModels;

namespace Cliptoo.UI.Services
{
    public interface IPreviewManager
    {
        ClipViewModel? PreviewClip { get; }
        bool IsPreviewOpen { get; set; }
        System.Windows.Controls.Primitives.PlacementMode PlacementMode { get; }
        UIElement? PlacementTarget { get; }

        void RequestShowPreview(ClipViewModel? clipVm);
        void RequestHidePreview();
        void TogglePreviewForSelection(ClipViewModel selectedVm, UIElement? placementTarget);
    }
}