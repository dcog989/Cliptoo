# Cliptoo Project Overview

Cliptoo is a high-performance clipboard manager for Windows built with .NET 10 / C# / WPF. It aims for minimal resource usage while supporting thousands of clips.

## Tech Stack

- **Framework:** .NET 10 / C# / WPF
- **UI Library:** WPF-UI for modern, fluent interface
- **Database:** SQLite with FTS5 extension for full-text search
- **Text Editor:** AvalonEdit with syntax highlighting
- **Image Processing:** ImageSharp, SkiaSharp, Svg.Skia, Jxl.NET
- **Updates:** Velopack for auto-updates

## Project Structure

```
Cliptoo/
├── Cliptoo.Core/          # Business logic, services, database
│   ├── Configuration/     # Settings management
│   ├── Database/          # SQLite repository, FTS5 queries
│   ├── Interfaces/        # Service interfaces
│   ├── Native/            # P/Invoke, clipboard monitoring
│   └── Services/          # Core services (thumbnails, caching, etc.)
├── Cliptoo.UI/            # WPF presentation layer
│   ├── Controls/          # Custom WPF controls
│   ├── Converters/        # Value converters
│   ├── Services/          # UI-specific services
│   ├── ViewModels/        # MVVM view models
│   └── Views/             # XAML windows/pages
└── .agents/               # LLM instructions
```

## Entry Points

### Application Entry Point
- `Cliptoo.UI/App.xaml.cs` - Main entry, DI container setup, lifecycle management

### Core Controller
- `Cliptoo.Core/CliptooController.cs` - Central coordinator for clipboard monitoring, processing, and database operations

### Main UI
- `Cliptoo.UI/ViewModels/MainViewModel.cs` - Primary view model
- `Cliptoo.UI/Views/MainWindow.xaml` - Main application window

## Key Architecture

### Performance Optimizations
- Virtualized scrolling for thousands of clips
- LRU cache for thumbnails and metadata (`LruCache<TKey, TValue>`)
- Background processing with `SemaphoreSlim` locks
- Proactive maintenance when clip count exceeds threshold
- SQLite FTS5 for fast full-text search

### Caching Strategy
- Thumbnail cache in `%LocalAppData%\Cliptoo\ClipboardImageCache`
- LRU cache for runtime thumbnail/metadata caching
- Web metadata (favicons, titles) cached locally

### Event Handling
- `ClipboardMonitor` - Native Win32 clipboard change detection
- `CliptooController` - Processes clipboard events, handles errors gracefully
- Events propagate via `IClipDataService.ClipDeleted`, `ProcessingFailed`, etc.

### Data Flow
1. `ClipboardMonitor` detects clipboard change
2. `CliptooController.ProcessClipboardChange()` processes content
3. `ContentProcessor` classifies content type
4. `ClipDataService` saves to SQLite via `ClipRepository`
5. UI updated via `INotifyPropertyChanged` / `ObservableCollection`

## Coding Principles

- Use current coding standards and patterns.
- KISS, Occam's razor, DRY, YAGNI.
- Optimize for actual and perceived performance.
- Self-documenting code via clear naming of variables, functions, etc.
- Comments only for workarounds/complex logic.
- No magic numbers - use constants with descriptive names.
- **Do NOT create docs files** (summary, reference, testing, etc.) unless instructed.

## File System Access

### Allowed
- All folders / files unless excluded below.

### Disallowed
- `.docs/`, `.git/`, `node_modules/`
- `repomix.config.json`, `bun.lock`, `.repomixignore`

## Build Commands

- Build: `dotnet build Cliptoo.slnx -c Release`
- Run: `dotnet run --project Cliptoo.UI/Cliptoo.UI.csproj`
