namespace Cliptoo.UI.Helpers
{
    public static class FormatUtils
    {
        public static string FormatBytes(long bytes)
        {
            var suf = new[] { "B", "KB", "MB", "GB", "TB", "PB", "EB" };
            if (bytes == 0)
                return "0 " + suf[0];
            long absoluteBytes = Math.Abs(bytes);
            int place = Convert.ToInt32(Math.Floor(Math.Log(absoluteBytes, 1024)));
            double num = Math.Round(absoluteBytes / Math.Pow(1024, place));
            return (Math.Sign(bytes) * num) + " " + suf[place];
        }
    }
}