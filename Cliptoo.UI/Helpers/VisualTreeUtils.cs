using System.Windows;
using System.Windows.Media;

namespace Cliptoo.UI.Helpers
{
    internal static class VisualTreeUtils
    {
        public static T? FindVisualChild<T>(DependencyObject? obj) where T : DependencyObject
        {
            if (obj == null) return null;
            for (int i = 0; i < VisualTreeHelper.GetChildrenCount(obj); i++)
            {
                DependencyObject child = VisualTreeHelper.GetChild(obj, i);
                if (child is T dependencyObject)
                    return dependencyObject;
                T? childOfChild = FindVisualChild<T>(child);
                if (childOfChild != null)
                    return childOfChild;
            }
            return null;
        }

        public static T? FindVisualAncestor<T>(DependencyObject? d) where T : DependencyObject
        {
            while (d != null)
            {
                if (d is T target) { return target; }
                if (d is Visual || d is System.Windows.Media.Media3D.Visual3D)
                {
                    d = VisualTreeHelper.GetParent(d);
                }
                else
                {
                    d = LogicalTreeHelper.GetParent(d);
                }
            }
            return null;
        }

        public static bool HasVisualAncestor(DependencyObject? descendant, DependencyObject? ancestor)
        {
            if (descendant is not Visual)
            {
                return false;
            }

            if (ancestor == null)
                return false;

            DependencyObject? parent = descendant;
            while (parent != null)
            {
                if (parent == ancestor)
                    return true;
                parent = VisualTreeHelper.GetParent(parent);
            }
            return false;
        }
    }
}