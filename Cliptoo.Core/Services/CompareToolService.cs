using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using Microsoft.Win32;

namespace Cliptoo.Core.Services
{
    public class CompareToolService : ICompareToolService
    {
        private class ToolDefinition
        {
            public string ExeName { get; set; } = string.Empty;
            public string Args { get; set; } = string.Empty;
            public List<string> RelativePaths { get; set; } = new();
        }

        public (string? Path, string? Args) FindCompareTool()
        {
            var toolDefinitions = new List<ToolDefinition>
            {
                new() {
                    ExeName = "Code.exe", Args = "--diff",
                    RelativePaths = new List<string> { "Microsoft VS Code" }
                },
                new() {
                    ExeName = "BCompare.exe", Args = "",
                    RelativePaths = new List<string> { "Beyond Compare 5", "Beyond Compare 4", "Beyond Compare 3" }
                },
                new() {
                    ExeName = "WinMergeU.exe", Args = "",
                    RelativePaths = new List<string> { "WinMerge" }
                },
                new() {
                    ExeName = "p4merge.exe", Args = "",
                    RelativePaths = new List<string> { "Perforce" }
                },
                new() {
                    ExeName = "compare.exe", Args = "",
                    RelativePaths = new List<string> { "Araxis\\Araxis Merge" }
                },
                new() {
                    ExeName = "totalcmd64.exe", Args = "/S=C",
                    RelativePaths = new List<string> { "totalcmd" }
                },
                new() {
                    ExeName = "totalcmd.exe", Args = "/S=C",
                    RelativePaths = new List<string> { "totalcmd" }
                },
            };

            foreach (var tool in toolDefinitions)
            {
                string? foundPath = FindToolPath(tool);
                if (!string.IsNullOrEmpty(foundPath))
                {
                    return (foundPath, tool.Args);
                }
            }

            return (null, null);
        }

        public string GetArgsForPath(string toolPath)
        {
            if (string.IsNullOrWhiteSpace(toolPath)) return "";

            var exeName = Path.GetFileName(toolPath).ToUpperInvariant();
            if (exeName.Contains("code.exe", StringComparison.Ordinal)) return "--diff";
            if (exeName.StartsWith("totalcmd", StringComparison.Ordinal)) return "/S=C";
            return "";
        }

        private static string? FindToolPath(ToolDefinition tool)
        {
            // Strategy 1: Check App Paths registry key (most reliable)
            string? path = GetPathFromAppPaths(tool.ExeName);
            if (File.Exists(path))
            {
                return path;
            }

            // Strategy 2: Check common Program Files locations with known subdirectories
            var programFilesBases = new[]
            {
                Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles),
                Environment.GetFolderPath(Environment.SpecialFolder.ProgramFilesX86)
            }.Distinct(StringComparer.OrdinalIgnoreCase);

            foreach (var baseDir in programFilesBases)
            {
                foreach (var relativePath in tool.RelativePaths)
                {
                    path = Path.Combine(baseDir, relativePath, tool.ExeName);
                    if (File.Exists(path))
                    {
                        return path;
                    }
                }
            }

            // Strategy 3: Search PATH environment variable
            path = GetPathFromEnvironment(tool.ExeName);
            if (File.Exists(path))
            {
                return path;
            }

            // Strategy 4: Specific hardcoded/special folder checks (as a last resort)
            if (tool.ExeName.Equals("Code.exe", StringComparison.OrdinalIgnoreCase))
            {
                path = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Programs", "Microsoft VS Code", "Code.exe");
                if (File.Exists(path))
                {
                    return path;
                }
            }
            if (tool.ExeName.StartsWith("totalcmd", StringComparison.OrdinalIgnoreCase))
            {
                path = Path.Combine("C:\\", "totalcmd", tool.ExeName);
                if (File.Exists(path))
                {
                    return path;
                }
            }

            return null;
        }

        private static string? GetPathFromAppPaths(string exeName)
        {
            try
            {
                string appPathsKey = @"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\";
                using (RegistryKey? key = Registry.LocalMachine.OpenSubKey(appPathsKey + exeName))
                {
                    if (key != null)
                    {
                        var value = key.GetValue(null)?.ToString();
                        return string.IsNullOrEmpty(value) ? null : Environment.ExpandEnvironmentVariables(value);
                    }
                }
            }
            catch (System.Security.SecurityException) { /* Ignore security or other errors */ }
            catch (IOException) { /* Ignore security or other errors */ }
            return null;
        }

        private static string? GetPathFromEnvironment(string exeName)
        {
            string? pathVar = Environment.GetEnvironmentVariable("PATH");
            if (pathVar != null)
            {
                var paths = pathVar.Split(Path.PathSeparator);
                foreach (var path in paths)
                {
                    try
                    {
                        string fullPath = Path.Combine(path, exeName);
                        if (File.Exists(fullPath))
                        {
                            return fullPath;
                        }
                    }
                    catch (ArgumentException) { /* Invalid characters in path */ }
                }
            }
            return null;
        }
    }
}