using Cliptoo.Core.Configuration;

namespace Cliptoo.UI.Services
{
    public record ClipDeletionRequested(int ClipId);
    public record ClipPinToggled(int ClipId, bool IsPinned);
    public record ClipMoveToTopRequested(int ClipId);
    public record ClipEditRequested(int ClipId);
    public record ClipOpenRequested(int ClipId);
    public record ClipSelectForCompareLeft(int ClipId);
    public record ClipCompareWithSelectedRight(int ClipId);
    public record ClipSendToRequested(int ClipId, SendToTarget Target);
    public record ClipTransformAndPasteRequested(int ClipId, string TransformType);
    public record ClipPasteRequested(int ClipId, bool? ForcePlainText);
    public record TogglePreviewForSelectionRequested(object? PlacementTarget);
}