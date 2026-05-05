using System.Windows;

namespace WuwaIDLauncher;

public partial class ConfirmDialog : Window
{
    public bool Confirmed { get; private set; }

    public ConfirmDialog(string message, Window owner)
    {
        InitializeComponent();
        MessageText.Text = message;
        Owner = owner;
    }

    void BtnOk_Click(object sender, RoutedEventArgs e)
    {
        Confirmed = true;
        Close();
    }

    void BtnCancel_Click(object sender, RoutedEventArgs e)
    {
        Confirmed = false;
        Close();
    }
}
