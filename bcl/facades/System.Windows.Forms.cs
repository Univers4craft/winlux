// Façade de compilation « System.Windows.Forms » pour les témoins de la BCL
// Winlux (#2753, #2764, #2795). Licence GPL 3. Jamais chargée ni livrée : un témoin
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
        public static void DoEvents() { throw new InvalidOperationException("façade"); }
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
        public virtual bool AutoSize { get { throw new InvalidOperationException("façade"); } set { throw new InvalidOperationException("façade"); } }
        public virtual System.Drawing.Size MinimumSize { get { throw new InvalidOperationException("façade"); } set { throw new InvalidOperationException("façade"); } }
        public virtual System.Drawing.Size MaximumSize { get { throw new InvalidOperationException("façade"); } set { throw new InvalidOperationException("façade"); } }
        public virtual System.Drawing.Size GetPreferredSize(System.Drawing.Size proposedSize) { throw new InvalidOperationException("façade"); }
        public System.Drawing.Size PreferredSize { get { throw new InvalidOperationException("façade"); } }
        public System.Drawing.Point Location { get { throw new InvalidOperationException("façade"); } set { throw new InvalidOperationException("façade"); } }
        public int Width { get { throw new InvalidOperationException("façade"); } set { throw new InvalidOperationException("façade"); } }
        public int Height { get { throw new InvalidOperationException("façade"); } set { throw new InvalidOperationException("façade"); } }
        public bool Enabled { get { throw new InvalidOperationException("façade"); } set { throw new InvalidOperationException("façade"); } }
        public bool Focused { get { throw new InvalidOperationException("façade"); } }
        public Control Parent { get { throw new InvalidOperationException("façade"); } set { throw new InvalidOperationException("façade"); } }
        public ControlCollection Controls { get { throw new InvalidOperationException("façade"); } }
        public event EventHandler Click { add { throw new InvalidOperationException("façade"); } remove { throw new InvalidOperationException("façade"); } }
        public static Control FromHandle(IntPtr handle) { throw new InvalidOperationException("façade"); }
        public static Keys ModifierKeys { get { throw new InvalidOperationException("façade"); } }
        public Form FindForm() { throw new InvalidOperationException("façade"); }
        public virtual bool PreProcessMessage(ref Message msg) { throw new InvalidOperationException("façade"); }
        protected virtual bool ProcessCmdKey(ref Message msg, Keys keyData) { throw new InvalidOperationException("façade"); }
        protected virtual bool ProcessDialogKey(Keys keyData) { throw new InvalidOperationException("façade"); }
        protected virtual void OnClick(EventArgs e) { throw new InvalidOperationException("façade"); }
        public class ControlCollection
        {
            public virtual void Add(Control value) { throw new InvalidOperationException("façade"); }
            public int Count { get { throw new InvalidOperationException("façade"); } }
        }
    }

    public struct Message
    {
        public IntPtr HWnd { get { throw new InvalidOperationException("façade"); } set { throw new InvalidOperationException("façade"); } }
        public int Msg { get { throw new InvalidOperationException("façade"); } set { throw new InvalidOperationException("façade"); } }
        public IntPtr WParam { get { throw new InvalidOperationException("façade"); } set { throw new InvalidOperationException("façade"); } }
        public IntPtr LParam { get { throw new InvalidOperationException("façade"); } set { throw new InvalidOperationException("façade"); } }
    }

    [Flags]
    public enum Keys { None = 0, Enter = 13, Return = 13, Escape = 27, Tab = 9, A = 65, KeyCode = 0xFFFF, Shift = 0x10000, Control = 0x20000, Alt = 0x40000 }

    public interface IButtonControl
    {
        DialogResult DialogResult { get; set; }
        void NotifyDefault(bool value);
        void PerformClick();
    }

    public class Label : Control
    {
        public Label() { throw new InvalidOperationException("façade"); }
        public override bool AutoSize { get { throw new InvalidOperationException("façade"); } set { throw new InvalidOperationException("façade"); } }
        public override System.Drawing.Size GetPreferredSize(System.Drawing.Size proposedSize) { throw new InvalidOperationException("façade"); }
    }

    public class ButtonBase : Control
    {
        protected ButtonBase() { throw new InvalidOperationException("façade"); }
        public override bool AutoSize { get { throw new InvalidOperationException("façade"); } set { throw new InvalidOperationException("façade"); } }
        public override System.Drawing.Size GetPreferredSize(System.Drawing.Size proposedSize) { throw new InvalidOperationException("façade"); }
    }

    public class Button : ButtonBase, IButtonControl
    {
        public Button() { throw new InvalidOperationException("façade"); }
        public virtual DialogResult DialogResult { get { throw new InvalidOperationException("façade"); } set { throw new InvalidOperationException("façade"); } }
        public virtual void NotifyDefault(bool value) { throw new InvalidOperationException("façade"); }
        public void PerformClick() { throw new InvalidOperationException("façade"); }
        protected override void OnClick(EventArgs e) { throw new InvalidOperationException("façade"); }
    }

    public enum ProgressBarStyle { Blocks = 0, Continuous = 1, Marquee = 2 }

    public class ProgressBar : Control
    {
        public ProgressBar() { throw new InvalidOperationException("façade"); }
        public int Minimum { get { throw new InvalidOperationException("façade"); } set { throw new InvalidOperationException("façade"); } }
        public int Maximum { get { throw new InvalidOperationException("façade"); } set { throw new InvalidOperationException("façade"); } }
        public int Value { get { throw new InvalidOperationException("façade"); } set { throw new InvalidOperationException("façade"); } }
        public int Step { get { throw new InvalidOperationException("façade"); } set { throw new InvalidOperationException("façade"); } }
        public ProgressBarStyle Style { get { throw new InvalidOperationException("façade"); } set { throw new InvalidOperationException("façade"); } }
        public void Increment(int value) { throw new InvalidOperationException("façade"); }
        public void PerformStep() { throw new InvalidOperationException("façade"); }
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
        public new System.Drawing.Size ClientSize { get { throw new InvalidOperationException("façade"); } set { throw new InvalidOperationException("façade"); } }
        public new System.Drawing.Size Size { get { throw new InvalidOperationException("façade"); } set { throw new InvalidOperationException("façade"); } }
        public DialogResult DialogResult { get { throw new InvalidOperationException("façade"); } set { throw new InvalidOperationException("façade"); } }
        public IButtonControl AcceptButton { get { throw new InvalidOperationException("façade"); } set { throw new InvalidOperationException("façade"); } }
        public IButtonControl CancelButton { get { throw new InvalidOperationException("façade"); } set { throw new InvalidOperationException("façade"); } }
        protected override bool ProcessCmdKey(ref Message msg, Keys keyData) { throw new InvalidOperationException("façade"); }
        protected override bool ProcessDialogKey(Keys keyData) { throw new InvalidOperationException("façade"); }
        protected override void OnLayout(LayoutEventArgs levent) { throw new InvalidOperationException("façade"); }
    }
}
