using Cliptoo.UI.Helpers;
using System.Windows;
using System.Windows.Controls;

namespace Cliptoo.UI.Controls.Tooltips
{
    public partial class TextPreviewTooltip : UserControl
    {
        public TextPreviewTooltip()
        {
            InitializeComponent();
            DebugUtils.LogMemoryUsage("TextPreviewTooltip Constructor");
            Loaded += (s, e) => DebugUtils.LogMemoryUsage("TextPreviewTooltip Loaded");
        }
    }
}