using Cliptoo.Core.Configuration;

namespace Cliptoo.UI.Services
{
    internal record ClipDeletionRequested(int ClipId);
    internal record ClipFavoriteToggled(int ClipId, bool IsFavorite);
    internal record ClipMoveToTopRequested(int ClipId);
    internal record ClipEditRequested(int ClipId);
    internal record ClipOpenRequested(int ClipId);
    internal record ClipSelectForCompareLeft(int ClipId);
    internal record ClipCompareWithSelectedRight(int ClipId);
    internal record ClipSendToRequested(int ClipId, SendToTarget Target);
    internal record ClipTransformAndPasteRequested(int ClipId, string TransformType);
    internal record ClipPasteRequested(int ClipId, bool? ForcePlainText);
    internal record ClipPasteFilePathRequested(int ClipId);
    internal record TogglePreviewForSelectionRequested(object? PlacementTarget);
    internal record CachesClearedMessage;
}