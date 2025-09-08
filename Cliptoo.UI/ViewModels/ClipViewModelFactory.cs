using Cliptoo.Core.Configuration;
using Cliptoo.Core.Database.Models;
using Cliptoo.Core.Interfaces;
using Cliptoo.Core.Services;
using Cliptoo.UI.Services;

namespace Cliptoo.UI.ViewModels
{
    public interface IClipViewModelFactory
    {
        ClipViewModel Create(Clip clip, Settings settings, string theme, MainViewModel mainViewModel);
    }

    public class ClipViewModelFactory : IClipViewModelFactory
    {
        private readonly IClipDetailsLoader _clipDetailsLoader;
        private readonly IIconProvider _iconProvider;
        private readonly IClipDataService _clipDataService;
        private readonly IThumbnailService _thumbnailService;
        private readonly IWebMetadataService _webMetadataService;

        public ClipViewModelFactory(
            IClipDetailsLoader clipDetailsLoader,
            IIconProvider iconProvider,
            IClipDataService clipDataService,
            IThumbnailService thumbnailService,
            IWebMetadataService webMetadataService)
        {
            _clipDetailsLoader = clipDetailsLoader;
            _iconProvider = iconProvider;
            _clipDataService = clipDataService;
            _thumbnailService = thumbnailService;
            _webMetadataService = webMetadataService;
        }

        public ClipViewModel Create(Clip clip, Settings settings, string theme, MainViewModel mainViewModel)
        {
            ArgumentNullException.ThrowIfNull(settings);

            var vm = new ClipViewModel(clip, mainViewModel, _clipDetailsLoader, _iconProvider, _clipDataService, _thumbnailService, _webMetadataService);
            vm.UpdateClip(clip, theme);
            return vm;
        }

    }
}