using System.Windows;

namespace WuwaIDLauncher
{
    public partial class SplashWindow : Window
    {
        public SplashWindow() => InitializeComponent();

        public void FadeOutAndClose() => Close();
    }
}
