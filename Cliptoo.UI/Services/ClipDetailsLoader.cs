using System.IO;
using System.Text;
using Cliptoo.Core;
using Cliptoo.Core.Configuration;
using Cliptoo.Core.Services;
using Cliptoo.UI.Helpers;
using Cliptoo.UI.ViewModels;
using SixLabors.ImageSharp;

namespace Cliptoo.UI.Services
{
    public class ClipDetailsLoader : IClipDetailsLoader
    {
        private readonly IImageDecoder _imageDecoder;

        public ClipDetailsLoader(IImageDecoder imageDecoder)
        {
            _imageDecoder = imageDecoder;
        }

        public async Task<string?> GetThumbnailAsync(ClipViewModel vm, IThumbnailService thumbnailService, IWebMetadataService webMetadataService, string theme)
        {
            var extension = Path.GetExtension(vm.Content)?.ToLowerInvariant() ?? string.Empty;

            if (vm.ClipType == AppConstants.ClipTypes.Image)
            {
                return await thumbnailService.GetThumbnailAsync(vm.Content, extension == ".svg" ? theme : null).ConfigureAwait(false);
            }
            if (vm.ClipType == AppConstants.ClipTypes.Link && Uri.TryCreate(vm.Content, UriKind.Absolute, out var uri))
            {
                return await webMetadataService.GetFaviconAsync(uri).ConfigureAwait(false);
            }
            return null;
        }

        public async Task<string?> GetImagePreviewAsync(ClipViewModel vm, IThumbnailService thumbnailService, uint size, string theme)
        {
            if (!vm.IsImage) return null;

            var extension = Path.GetExtension(vm.Content)?.ToLowerInvariant() ?? string.Empty;
            return await thumbnailService.GetImagePreviewAsync(vm.Content, size, extension == ".svg" ? theme : null);
        }

        public async Task<string?> GetPageTitleAsync(ClipViewModel vm, IWebMetadataService webMetadataService, CancellationToken token)
        {
            if (!vm.IsLinkToolTip || string.IsNullOrEmpty(vm.Content) || !Uri.TryCreate(vm.Content, UriKind.Absolute, out var uri)) return null;

            try
            {
                var title = await webMetadataService.GetPageTitleAsync(uri).ConfigureAwait(false);
                if (!token.IsCancellationRequested)
                {
                    return string.IsNullOrWhiteSpace(title) ? vm.DisplayContent : title;
                }
            }
            catch (Exception ex)
            {
                if (!token.IsCancellationRequested)
                {
                    LogManager.Log(ex, "Failed to load page title.");
                    return vm.DisplayContent;
                }
            }

            return null;
        }

        public async Task<(string? properties, string? typeInfo, bool isMissing)> GetFilePropertiesAsync(ClipViewModel vm, CancellationToken token)
        {
            if (!vm.IsFileBased || string.IsNullOrEmpty(vm.Content))
            {
                return (null, null, false);
            }
            DebugUtils.LogMemoryUsage($"GetFilePropertiesAsync START (Path: {vm.Content})");

            string? fileProperties = null;
            string? fileTypeInfo = null;
            bool isMissing = false;

            try
            {
                var path = vm.Content.Trim();
                var sb = new StringBuilder();

                await Task.Run(async () =>
                {
                    if (token.IsCancellationRequested) return;
                    DebugUtils.LogMemoryUsage("GetFilePropertiesAsync - Before file system access");

                    if (Directory.Exists(path))
                    {
                        DebugUtils.LogMemoryUsage("GetFilePropertiesAsync - Directory.Exists passed");
                        var dirInfo = new DirectoryInfo(path);
                        DebugUtils.LogMemoryUsage("GetFilePropertiesAsync - After new DirectoryInfo()");
                        sb.AppendLine($"Modified: {dirInfo.LastWriteTime:yyyy-MM-dd HH:mm}");
                        fileTypeInfo = FormatUtils.GetFriendlyClipTypeName(vm.ClipType);
                        try
                        {
                            DebugUtils.LogMemoryUsage("GetFilePropertiesAsync - Before CalculateDirectorySize");
                            var dirSize = await Task.Run(() => CalculateDirectorySize(dirInfo, token), token);
                            DebugUtils.LogMemoryUsage("GetFilePropertiesAsync - After CalculateDirectorySize");
                            if (token.IsCancellationRequested) return;
                            sb.AppendLine($"Size: {FormatUtils.FormatBytes(dirSize.Size)}");
                            sb.AppendLine($"Contains: {dirSize.FileCount} files, {dirSize.FolderCount} folders");
                        }
                        catch (OperationCanceledException)
                        {
                            return;
                        }
                        catch (Exception)
                        {
                            sb.AppendLine("Size: (access denied)");
                        }
                    }
                    else if (File.Exists(path))
                    {
                        DebugUtils.LogMemoryUsage("GetFilePropertiesAsync - File.Exists passed");
                        var fileInfo = new FileInfo(path);
                        DebugUtils.LogMemoryUsage("GetFilePropertiesAsync - After new FileInfo()");
                        sb.AppendLine($"Size: {FormatUtils.FormatBytes(fileInfo.Length)}");
                        sb.AppendLine($"Modified: {fileInfo.LastWriteTime:yyyy-MM-dd HH:mm}");
                        fileTypeInfo = $"{fileInfo.Extension.ToLower()} ({FormatUtils.GetFriendlyClipTypeName(vm.ClipType)})";

                        if (vm.IsImage)
                        {
                            try
                            {
                                var extension = Path.GetExtension(path).ToLowerInvariant();
                                using var stream = File.OpenRead(path);

                                if (extension == ".jxl")
                                {
                                    using var image = await _imageDecoder.DecodeAsync(stream, extension);
                                    if (image != null)
                                    {
                                        sb.AppendLine($"Dimensions: {image.Width} x {image.Height}");
                                    }
                                }
                                else
                                {
                                    var imageInfo = await Image.IdentifyAsync(stream, token);
                                    if (imageInfo != null)
                                    {
                                        sb.AppendLine($"Dimensions: {imageInfo.Width} x {imageInfo.Height}");
                                    }
                                }
                            }
                            catch { /* Ignore */ }
                        }
                    }
                    else
                    {
                        isMissing = true;
                    }

                    if (!token.IsCancellationRequested && !isMissing)
                    {
                        fileProperties = sb.ToString().Trim();
                    }
                }, token);
            }
            catch (Exception ex)
            {
                if (!token.IsCancellationRequested)
                {
                    fileProperties = "Error reading properties.";
                }
                LogManager.Log(ex, "Failed to load file properties.");
            }

            return (fileProperties, fileTypeInfo, isMissing);
        }

        private (long Size, int FileCount, int FolderCount) CalculateDirectorySize(DirectoryInfo dirInfo, CancellationToken token)
        {
            long size = 0;
            int fileCount = 0;
            int folderCount = 0;

            try
            {
                foreach (var file in dirInfo.EnumerateFiles())
                {
                    token.ThrowIfCancellationRequested();
                    size += file.Length;
                    fileCount++;
                }

                foreach (var dir in dirInfo.EnumerateDirectories())
                {
                    token.ThrowIfCancellationRequested();
                    var subDirSize = CalculateDirectorySize(dir, token);
                    size += subDirSize.Size;
                    fileCount += subDirSize.FileCount;
                    folderCount += subDirSize.FolderCount + 1; // +1 for the current subdirectory
                }
            }
            catch (UnauthorizedAccessException) { /* ignore */ }

            return (size, fileCount, folderCount);
        }

    }
}