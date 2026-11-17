// Façade de compilation « System.Windows.Forms » pour les témoins de la BCL
// Winlux (#2753). Licence GPL 3. Jamais chargée ni livrée : un témoin compilé
// contre elle appelle MessageBox.Show dans l'assembly « System.Windows.Forms »,
// comme une appli WinForms .NET Framework. Valeurs des énumérations : celles
// documentées sur Learn (System.Windows.Forms.MessageBoxButtons, etc.).

[assembly: System.Reflection.AssemblyVersion("4.0.0.0")]

namespace System.Windows.Forms
{
    public interface IWin32Window
    {
        IntPtr Handle { get; }
    }

    public enum DialogResult { None = 0, OK = 1, Cancel = 2, Abort = 3, Retry = 4, Ignore = 5, Yes = 6, No = 7 }

    public enum MessageBoxButtons { OK = 0, OKCancel = 1, AbortRetryIgnore = 2, YesNoCancel = 3, YesNo = 4, RetryCancel = 5 }

    public enum MessageBoxIcon
    {
        None = 0, Hand = 16, Stop = 16, Error = 16, Question = 32, Exclamation = 48, Warning = 48,
        Asterisk = 64, Information = 64
    }

    public enum MessageBoxDefaultButton { Button1 = 0, Button2 = 256, Button3 = 512 }

    public enum MessageBoxOptions
    {
        DefaultDesktopOnly = 0x20000, RightAlign = 0x80000, RtlReading = 0x100000, ServiceNotification = 0x200000
    }

    public class MessageBox
    {
        public static DialogResult Show(string text) { throw new InvalidOperationException("façade"); }
        public static DialogResult Show(string text, string caption) { throw new InvalidOperationException("façade"); }
        public static DialogResult Show(string text, string caption, MessageBoxButtons buttons)
        { throw new InvalidOperationException("façade"); }
        public static DialogResult Show(string text, string caption, MessageBoxButtons buttons, MessageBoxIcon icon)
        { throw new InvalidOperationException("façade"); }
        public static DialogResult Show(string text, string caption, MessageBoxButtons buttons, MessageBoxIcon icon,
            MessageBoxDefaultButton defaultButton) { throw new InvalidOperationException("façade"); }
        public static DialogResult Show(IWin32Window owner, string text, string caption, MessageBoxButtons buttons,
            MessageBoxIcon icon, MessageBoxDefaultButton defaultButton) { throw new InvalidOperationException("façade"); }
        public static DialogResult Show(string text, string caption, MessageBoxButtons buttons, MessageBoxIcon icon,
            MessageBoxDefaultButton defaultButton, MessageBoxOptions options, bool displayHelpButton)
        { throw new InvalidOperationException("façade"); }
    }
}
