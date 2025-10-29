using System.Collections.ObjectModel;
using Cliptoo.UI.ViewModels;

namespace Cliptoo.UI.Services
{
    public interface IClipDisplayService
    {
        ObservableCollection<ClipViewModel> Clips { get; }
        ObservableCollection<FilterOption> FilterOptions { get; }
        string SearchTerm { get; set; }
        FilterOption SelectedFilter { get; set; }
        bool IsLoading { get; }

        event EventHandler? ListScrolledToTopRequest;

        Task InitializeAsync();
        Task LoadClipsAsync(bool scrollToTop = false);
        Task LoadMoreClipsAsync();
        void RefreshClipList();
        void ClearClipsForHiding();
    }
}