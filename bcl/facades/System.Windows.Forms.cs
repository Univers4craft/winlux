// Façade de compilation « System.Windows.Forms » pour les témoins de la BCL
// Winlux (#2753, #2764). Licence GPL 3. Jamais chargée ni livrée : un témoin
// compilé contre elle appelle MessageBox, Application et Form dans
// l'assembly « System.Windows.Forms », comme une appli WinForms .NET
// Framework ; le moteur les trouve dans notre mscorlib (ou, pour MessageBox,
// parmi ses appels internes). Valeurs des énumérations : celles documentées
// sur Learn. Les structures de System.Drawing viennent de notre mscorlib.

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

    public enum FormBorderStyle { None = 0, FixedSingle = 1, Fixed3D = 2, FixedDialog = 3, Sizable = 4, FixedToolWindow = 5, SizableToolWindow = 6 }

    public enum FormStartPosition { Manual = 0, CenterScreen = 1, WindowsDefaultLocation = 2, WindowsDefaultBounds = 3, CenterParent = 4 }

    public enum CloseReason
    {
        None = 0, WindowsShutDown = 1, MdiFormClosing = 2, UserClosing = 3, TaskManagerClosing = 4,
        FormOwnerClosing = 5, ApplicationExitCall = 6
    }

    public class LayoutEventArgs : EventArgs
    {
        public LayoutEventArgs(Control affectedControl, string affectedProperty) { throw new InvalidOperationException("façade"); }
        public string AffectedProperty { get { throw new InvalidOperationException("façade"); } }
    }

    public class FormClosingEventArgs : System.ComponentModel.CancelEventArgs
    {
        public FormClosingEventArgs(CloseReason closeReason, bool cancel) { throw new InvalidOperationException("façade"); }
        public CloseReason CloseReason { get { throw new InvalidOperationException("façade"); } }
    }

    public sealed class Application
    {
        private Application() { }
        public static void EnableVisualStyles() { throw new InvalidOperationException("façade"); }
        public static void SetCompatibleTextRenderingDefault(bool defaultValue) { throw new InvalidOperationException("façade"); }
        public static void Run(Form mainForm) { throw new InvalidOperationException("façade"); }
        public static void Exit() { throw new InvalidOperationException("façade"); }
    }

    public class Control : System.ComponentModel.Component, IWin32Window
    {
        public IntPtr Handle { get { throw new InvalidOperationException("façade"); } }
        public bool IsHandleCreated { get { throw new InvalidOperationException("façade"); } }
        public bool IsDisposed { get { throw new InvalidOperationException("façade"); } }
        public virtual string Text { get { throw new InvalidOperationException("façade"); } set { throw new InvalidOperationException("façade"); } }
        public bool Visible { get { throw new InvalidOperationException("façade"); } set { throw new InvalidOperationException("façade"); } }
        public System.Drawing.Size Size { get { throw new InvalidOperationException("façade"); } set { throw new InvalidOperationException("façade"); } }
        public System.Drawing.Size ClientSize { get { throw new InvalidOperationException("façade"); } set { throw new InvalidOperationException("façade"); } }
        public void Show() { throw new InvalidOperationException("façade"); }
        public void Hide() { throw new InvalidOperationException("façade"); }
        public void SuspendLayout() { throw new InvalidOperationException("façade"); }
        public void ResumeLayout(bool performLayout) { throw new InvalidOperationException("façade"); }
        public void PerformLayout() { throw new InvalidOperationException("façade"); }
        protected virtual void OnLayout(LayoutEventArgs levent) { throw new InvalidOperationException("façade"); }
    }

    public class ScrollableControl : Control { }

    public class ContainerControl : ScrollableControl { }

    public class Form : ContainerControl
    {
        public Form() { throw new InvalidOperationException("façade"); }
        public event EventHandler Shown { add { throw new InvalidOperationException("façade"); } remove { throw new InvalidOperationException("façade"); } }
        public FormBorderStyle FormBorderStyle { get { throw new InvalidOperationException("façade"); } set { throw new InvalidOperationException("façade"); } }
        public FormStartPosition StartPosition { get { throw new InvalidOperationException("façade"); } set { throw new InvalidOperationException("façade"); } }
        public bool MaximizeBox { get { throw new InvalidOperationException("façade"); } set { throw new InvalidOperationException("façade"); } }
        public void Close() { throw new InvalidOperationException("façade"); }
        protected virtual void OnFormClosing(FormClosingEventArgs e) { throw new InvalidOperationException("façade"); }
    }
}
