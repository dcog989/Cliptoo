# Cliptoo Architecture Summary

This document outlines the software architecture of Cliptoo, a high-performance Windows clipboard manager.

## Core Technologies

- **Framework**: .NET9 / C#
- **Database**: SQLite / FTS5
- **Build System**: Visual Studio 2022
- **Key Packages**:
  - <https://www.nuget.org/packages/WPF-UI> / <https://wpfui.lepo.co/api/Wpf.Ui.html> / <https://wpfui.lepo.co/documentation/getting-started.html> / <https://github.com/lepoco/wpfui/tree/main/samples>
  - <https://www.nuget.org/packages/WPF-UI.Tray>
  - <https://www.nuget.org/packages/Svg.Skia/>
  - <https://www.nuget.org/packages/AvalonEdit>

## System Architecture

Cliptoo follows a decoupled design with a separate backend and UI. The **backend** is a service layer handling data, logic, and system integration, while the **UI** is a client for presentation. A singleton controller acts as the API between these two layers.

## Backend Logic

The backend is non-UI, efficient, and uses background threads for intensive operations.

- **Database Management:** Uses high-concurrency **Write-Ahead Logging** to store clip data (content, type, timestamp, status). Features include **full-text search** via FTS5 and automatic clean-up routines.
- **Native System Integration:** Monitors the clipboard with an **event-driven** approach, registers a global hotkey, and can simulate "paste" commands. It also manages a system tray icon.
- **Content Processing:** A dedicated thread pool handles asynchronous tasks like thumbnail generation and clip processing. It classifies clips using a `filetypes.json` and refines text-based clips into specific types (e.g., links, color codes, JSON). It also generates syntax-highlighted rich-text for code clips.
- **Configuration & Logging:** User preferences (themes, hotkeys) are persisted to a local **JSON file**. A custom message handler logs application events and errors to a persistent log file for diagnostics.

## User Interface

The UI is a lightweight client focused on data presentation and user interaction.

- **Main View:** A single window with a control bar and a **virtualized** clip list for smooth scrolling. Search and filter inputs are **debounced** to prevent excessive updates.
- **Clip Representation:** Clips are visually distinct with icons, and image clips show thumbnails. Active search terms are highlighted. Color codes are displayed as swatches.
- **Interaction Model:** Fully navigable by keyboard. A right-click context menu offers actions like Pin/Unpin, Delete, and text transformations.
- **Window Management:** Window sizes and positions are persistent, and new windows are intelligently positioned to remain on-screen.
- **Theming & Feedback:** Supports light, dark, and system themes, with a customizable accent color, font, and padding. A non-intrusive notification system provides user feedback.
