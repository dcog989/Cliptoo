# ![Cliptoo logo](./Cliptoo.UI/Assets/Icons/cliptoo-64.svg) Cliptoo: The Advanced Clipboard Manager

Cliptoo is a high-performance clipboard manager for Windows, filled with the features you need - as well as some you didn't know you needed.

It has been carefully designed to remain performant whether you have 10 clips or 10,000 saved. With the fuzzy search and filter system, you can easily find the clip you need, and confirm it with preview tooltip for text, colors, images, URLs, etc.

[Download the latest release now](./Cliptoo/releases/latest/).

![License](https://img.shields.io/github/license/dcog989/cliptoo?style=for-the-badge)
![Latest Release](https://img.shields.io/github/v/release/dcog989/cliptoo?style=for-the-badge)

---

## Screenshots

<!-- TODO: Add a high-quality GIF or screenshot of the application in action -->
*(A preview of Cliptoo's interface, showing the virtualized list, search highlighting, and a color preview tooltip)*

## Features

### Advanced Clipboard Management

* **Persistent History:** Never lose a copied item again. Your history is stored locally in a high-performance SQLite database.
* **Full-Text Search:** Instantly find clips with a powerful, debounced search engine that highlights matches in real-time.
* **Content-Aware Filtering:** Quickly filter your history by type: text, links, images, colors, pinned items, etc.
* **Pinning:** Keep frequently used items, don't delete them with the auto-cleanup.
* **Virtualized Scrolling:** Scroll through thousands of clips with no performance degradation.

### Content-Aware Previews

* **Image Previews:** Hover to see thumbnails and detailed previews for all common image formats, including `.svg`, `.webp`, and `.jxl`.
* **Color Swatches:** Color codes (`#hex`, `rgb()`, `hsl()`, `oklch()`) are automatically detected and displayed as color swatches with transparency support.
* **Code Snippet Highlighting:** Code blocks are automatically identified and syntax-highlighted in tooltips and the built-in editor, powered by AvalonEdit.
* **Web Link Metadata:** Automatically fetches and displays the page title for web links.
* **File & Folder Information:** See file size, modification date, and type information for file-based clips directly in the tooltip.

### Powerful Tools & Integrations

* **Quick Paste:** Hold a modifier key (e.g., `Ctrl+Alt`) to overlay numbered shortcuts on your clips for lightning-fast pasting.
* **Text Transformations:** Instantly transform and paste text with a wide range of actions:
  * Case changes: `UPPER`, `lower`, `Capitalize`, `Sentence`, `iNVERT`.
  * Whitespace control: `Trim`, `Remove Line Feeds`.
  * Code formatting: `kebab-case`, `snake_case`, `CamelCase`.
* **"Send To" Integration:** Configure custom context menu actions to send clip content directly to your favorite external applications.
* **Text Compare:** Select two text-based clips and launch an external comparison tool (like VS Code, Beyond Compare, or WinMerge) to see the differences.

### Deep Customization

* **Modern Theming:** Choose between Light, Dark, or sync with the Windows system theme. The UI uses modern Mica backdrop effects.
* **Custom Accent Colors:** Fine-tune the application's accent color with a hue slider and choose between vibrant or muted saturation levels.
* **Typography Control:** Customize the font family, font size, and item padding for both the main list and preview tooltips to create your perfect layout.
* **Configurable Hotkeys:** Set global hotkeys for showing the main window, activating quick paste, and toggling the preview tooltip.

### Robust Data Management

* **Automatic Cleanup:** Configure rules to automatically delete old clips based on age or total number of items, keeping your database lean.
* **Manual Tools:**
  * **Clear History:** Delete all unpinned items.
  * **Clear Caches:** Reclaim disk space by deleting all cached thumbnails and web metadata.
  * **Deadhead:** Remove clips that point to files or folders that no longer exist.
* **Database Statistics:** View detailed statistics about your clipboard usage, including total clips, paste count, and database size.

## Tech Stack

Cliptoo is built with a modern, performance-oriented technology stack:

* **Framework:** .NET 9 / C# / WPF
* **UI Library:** [WPF-UI](https://github.com/lepoco/wpfui) for a modern, fluent user interface.
* **Database:** SQLite with the FTS5 extension for high-speed full-text searching.
* **Text Editor:** [AvalonEdit](https://github.com/icsharpcode/AvalonEdit) for the integrated clip editor with syntax highlighting.
* **Image Processing:** [ImageSharp](https://github.com/SixLabors/ImageSharp) for robust image decoding and thumbnail generation.
* **Vector Graphics:** [SkiaSharp](https://github.com/mono/SkiaSharp) and [Svg.Skia](https://github.com/wieslawsoltes/Svg.Skia) for high-quality SVG rendering.

## Getting Started

1. Navigate to the [**Releases**](https://github.com/dcog989/cliptoo/releases) page.
2. Download the latest `.zip` file from the "Assets" section.
3. Extract the archive to a location of your choice.
4. Run `Cliptoo.UI.exe`.

The application is fully portable and requires no installation.

## Building from Source

### Prerequisites

* Visual Studio 2022
* .NET 9 SDK

### Steps

1. Clone the repository: `git clone https://github.com/dcog989/cliptoo.git`
2. Open `Cliptoo.sln` in Visual Studio.
3. Set the build configuration to `Release` and the platform to `x64`.
4. Build the solution. The output will be in `Cliptoo.UI/bin/Release/net9.0-windows/win-x64/`.

Alternatively, run the `build-production-run.ps1` PowerShell script to create an optimized, single-file portable executable in the `build/production` directory.

## Contributing

Contributions are welcome! If you have a feature request, bug report, or pull request, please feel free to open an issue or submit a PR.

## Acknowledgements

Cliptoo is built on the shoulders of giants. A special thank you to the creators and maintainers of these essential open-source projects:

* [WPF-UI](https://github.com/lepoco/wpfui)
* [AvalonEdit](https://github.com/icsharpcode/AvalonEdit)
* [ImageSharp](https://github.com/SixLabors/ImageSharp)
* [Jxl.NET](https://github.com/wsvincent/jxl.net)
* [SkiaSharp](https://github.com/mono/SkiaSharp) & [Svg.Skia](https://github.com/wieslawsoltes/Svg.Skia)
* [SQLitePCL.raw](https://github.com/ericsink/SQLitePCL.raw)

## License

This project is licensed under the [MIT License](LICENSE).
