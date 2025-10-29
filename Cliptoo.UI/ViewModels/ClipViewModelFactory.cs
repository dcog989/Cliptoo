using Cliptoo.Core.Database.Models;
using Cliptoo.Core.Interfaces;
using Cliptoo.Core.Services;
using Cliptoo.UI.Services;

namespace Cliptoo.UI.ViewModels
{
    public interface IClipViewModelFactory
    {
        ClipViewModel Create(Clip clip, string theme);
    }

    public class ClipViewModelFactory : IClipViewModelFactory
    {
        private readonly IClipDetailsLoader _clipDetailsLoader;
        private readonly IIconProvider _iconProvider;
        private readonly IClipDataService _clipDataService;
        private readonly IThumbnailService _thumbnailService;
        private readonly IWebMetadataService _webMetadataService;
        private readonly IEventAggregator _eventAggregator;
        private readonly IComparisonStateService _comparisonStateService;
        private readonly ISettingsService _settingsService;
        private readonly IUiSharedResources _sharedResources;
        private readonly IFontProvider _fontProvider;
        private readonly IPreviewManager _previewManager;

        public ClipViewModelFactory(
            IClipDetailsLoader clipDetailsLoader,
            IIconProvider iconProvider,
            IClipDataService clipDataService,
            IThumbnailService thumbnailService,
            IWebMetadataService webMetadataService,
            IEventAggregator eventAggregator,
            IComparisonStateService comparisonStateService,
            ISettingsService settingsService,
            IUiSharedResources sharedResources,
            IFontProvider fontProvider,
            IPreviewManager previewManager)
        {
            _clipDetailsLoader = clipDetailsLoader;
            _iconProvider = iconProvider;
            _clipDataService = clipDataService;
            _thumbnailService = thumbnailService;
            _webMetadataService = webMetadataService;
            _eventAggregator = eventAggregator;
            _comparisonStateService = comparisonStateService;
            _settingsService = settingsService;
            _sharedResources = sharedResources;
            _fontProvider = fontProvider;
            _previewManager = previewManager;
        }

        public ClipViewModel Create(Clip clip, string theme)
        {
            var vm = new ClipViewModel(
                clip,
                _clipDetailsLoader,
                _iconProvider,
                _clipDataService,
                _thumbnailService,
                _webMetadataService,
                _eventAggregator,
                _comparisonStateService,
                _settingsService,
                _sharedResources,
                _fontProvider,
                _previewManager
            );
            vm.UpdateClip(clip, theme);
            return vm;
        }
    }
}