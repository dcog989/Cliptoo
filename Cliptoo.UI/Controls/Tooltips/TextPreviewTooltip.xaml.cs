using System.Windows.Controls;
using Cliptoo.UI.Helpers;

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