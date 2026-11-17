// System.Windows.Forms de la mscorlib Winlux, mur 1 (#2764, suivi #2752) :
// Application, Control, ScrollableControl, ContainerControl et Form de base,
// sur de vraies fenêtres Win32 créées par nos user32 (P/Invoke), et les
// structures de System.Drawing qu'elles emploient (Point, Size, SizeF,
// Rectangle). Licence GPL 3.
//
// Écrit d'après la documentation publique de .NET Framework 4.x (MS Learn :
// « Application Class », « Control Class », « Form Class », « Form.Shown »,
// « Form.FormClosing », « FormBorderStyle », « FormStartPosition »,
// « CloseReason », « Message Structure », « System.Drawing.Size »…) et celle
// de l'API Win32 (RegisterClassExW, CreateWindowExW, AdjustWindowRectEx,
// GetMessageW…) ; aucun code de Microsoft, de Mono ou de Wine.
//
// Les contrôles enfants (Button, Label, ProgressBar), la mise à l'échelle,
// les polices et les icônes viennent aux murs suivants.
using System;
using System.Collections;
using System.Collections.Generic;
using System.ComponentModel;
using System.Drawing;
using System.Runtime.InteropServices;

namespace System.Drawing
{
    public struct Point
    {
        public static readonly Point Empty = new Point();
        private int _x, _y;
        public Point(int x, int y) { _x = x; _y = y; }
        public int X { get { return _x; } set { _x = value; } }
        public int Y { get { return _y; } set { _y = value; } }
        public bool IsEmpty { get { return _x == 0 && _y == 0; } }
        public static bool operator ==(Point a, Point b) { return a._x == b._x && a._y == b._y; }
        public static bool operator !=(Point a, Point b) { return !(a == b); }
        public override bool Equals(object obj) { return obj is Point && this == (Point)obj; }
        public override int GetHashCode() { return _x ^ (_y << 16); }
        public override string ToString() { return "{X=" + _x + ",Y=" + _y + "}"; }
    }

    public struct Size
    {
        public static readonly Size Empty = new Size();
        private int _w, _h;
        public Size(int width, int height) { _w = width; _h = height; }
        public Size(Point pt) { _w = pt.X; _h = pt.Y; }
        public int Width { get { return _w; } set { _w = value; } }
        public int Height { get { return _h; } set { _h = value; } }
        public bool IsEmpty { get { return _w == 0 && _h == 0; } }
        public static bool operator ==(Size a, Size b) { return a._w == b._w && a._h == b._h; }
        public static bool operator !=(Size a, Size b) { return !(a == b); }
        public static Size operator +(Size a, Size b) { return new Size(a._w + b._w, a._h + b._h); }
        public static Size operator -(Size a, Size b) { return new Size(a._w - b._w, a._h - b._h); }
        public override bool Equals(object obj) { return obj is Size && this == (Size)obj; }
        public override int GetHashCode() { return _w ^ (_h << 16); }
        public override string ToString() { return "{Width=" + _w + ", Height=" + _h + "}"; }
    }

    public struct SizeF
    {
        public static readonly SizeF Empty = new SizeF();
        private float _w, _h;
        public SizeF(float width, float height) { _w = width; _h = height; }
        public float Width { get { return _w; } set { _w = value; } }
        public float Height { get { return _h; } set { _h = value; } }
        public bool IsEmpty { get { return _w == 0 && _h == 0; } }
        public static bool operator ==(SizeF a, SizeF b) { return a._w == b._w && a._h == b._h; }
        public static bool operator !=(SizeF a, SizeF b) { return !(a == b); }
        public override bool Equals(object obj) { return obj is SizeF && this == (SizeF)obj; }
        public override int GetHashCode() { return (int)_w ^ ((int)_h << 16); }
        public Size ToSize() { return new Size((int)_w, (int)_h); }
    }

    public struct Rectangle
    {
        public static readonly Rectangle Empty = new Rectangle();
        private int _x, _y, _w, _h;
        public Rectangle(int x, int y, int width, int height) { _x = x; _y = y; _w = width; _h = height; }
        public Rectangle(Point location, Size size) { _x = location.X; _y = location.Y; _w = size.Width; _h = size.Height; }
        public int X { get { return _x; } set { _x = value; } }
        public int Y { get { return _y; } set { _y = value; } }
        public int Width { get { return _w; } set { _w = value; } }
        public int Height { get { return _h; } set { _h = value; } }
        public int Left { get { return _x; } }
        public int Top { get { return _y; } }
        public int Right { get { return _x + _w; } }
        public int Bottom { get { return _y + _h; } }
        public Point Location { get { return new Point(_x, _y); } set { _x = value.X; _y = value.Y; } }
        public Size Size { get { return new Size(_w, _h); } set { _w = value.Width; _h = value.Height; } }
        public bool IsEmpty { get { return _x == 0 && _y == 0 && _w == 0 && _h == 0; } }
        public bool Contains(int x, int y) { return x >= _x && x < _x + _w && y >= _y && y < _y + _h; }
        public static bool operator ==(Rectangle a, Rectangle b) { return a._x == b._x && a._y == b._y && a._w == b._w && a._h == b._h; }
        public static bool operator !=(Rectangle a, Rectangle b) { return !(a == b); }
        public override bool Equals(object obj) { return obj is Rectangle && this == (Rectangle)obj; }
        public override int GetHashCode() { return _x ^ (_y << 8) ^ (_w << 16) ^ (_h << 24); }
        public override string ToString() { return "{X=" + _x + ",Y=" + _y + ",Width=" + _w + ",Height=" + _h + "}"; }
    }
}

namespace System.ComponentModel
{
    public class CancelEventArgs : EventArgs
    {
        private bool _cancel;
        public CancelEventArgs() { }
        public CancelEventArgs(bool cancel) { _cancel = cancel; }
        public bool Cancel { get { return _cancel; } set { _cancel = value; } }
    }

    public delegate void CancelEventHandler(object sender, CancelEventArgs e);
}

namespace System.Windows.Forms
{
    public interface IWin32Window
    {
        IntPtr Handle { get; }
    }

    public enum FormBorderStyle { None = 0, FixedSingle = 1, Fixed3D = 2, FixedDialog = 3, Sizable = 4, FixedToolWindow = 5, SizableToolWindow = 6 }

    public enum FormStartPosition { Manual = 0, CenterScreen = 1, WindowsDefaultLocation = 2, WindowsDefaultBounds = 3, CenterParent = 4 }

    public enum FormWindowState { Normal = 0, Minimized = 1, Maximized = 2 }

    public enum CloseReason
    {
        None = 0, WindowsShutDown = 1, MdiFormClosing = 2, UserClosing = 3, TaskManagerClosing = 4,
        FormOwnerClosing = 5, ApplicationExitCall = 6
    }

    public enum AutoScaleMode { None = 0, Font = 1, Dpi = 2, Inherit = 3 }

    public struct Message
    {
        private IntPtr _hwnd, _wparam, _lparam, _result;
        private int _msg;
        public IntPtr HWnd { get { return _hwnd; } set { _hwnd = value; } }
        public int Msg { get { return _msg; } set { _msg = value; } }
        public IntPtr WParam { get { return _wparam; } set { _wparam = value; } }
        public IntPtr LParam { get { return _lparam; } set { _lparam = value; } }
        public IntPtr Result { get { return _result; } set { _result = value; } }

        public static Message Create(IntPtr hWnd, int msg, IntPtr wparam, IntPtr lparam)
        {
            Message m = new Message();
            m._hwnd = hWnd; m._msg = msg; m._wparam = wparam; m._lparam = lparam;
            return m;
        }
    }

    public class LayoutEventArgs : EventArgs
    {
        private readonly Control _affected;
        private readonly string _property;
        public LayoutEventArgs(Control affectedControl, string affectedProperty) { _affected = affectedControl; _property = affectedProperty; }
        public Control AffectedControl { get { return _affected; } }
        public string AffectedProperty { get { return _property; } }
    }

    public delegate void LayoutEventHandler(object sender, LayoutEventArgs e);

    public class FormClosingEventArgs : CancelEventArgs
    {
        private readonly CloseReason _reason;
        public FormClosingEventArgs(CloseReason closeReason, bool cancel) : base(cancel) { _reason = closeReason; }
        public CloseReason CloseReason { get { return _reason; } }
    }

    public delegate void FormClosingEventHandler(object sender, FormClosingEventArgs e);

    public class FormClosedEventArgs : EventArgs
    {
        private readonly CloseReason _reason;
        public FormClosedEventArgs(CloseReason closeReason) { _reason = closeReason; }
        public CloseReason CloseReason { get { return _reason; } }
    }

    public delegate void FormClosedEventHandler(object sender, FormClosedEventArgs e);

    // ── Appels Win32 (nos user32 et kernel32) ──

    internal delegate IntPtr ProcFenetre(IntPtr hwnd, int msg, IntPtr wparam, IntPtr lparam);

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    internal struct ClasseFenetre
    {
        public int cbSize;
        public int style;
        public ProcFenetre lpfnWndProc;
        public int cbClsExtra;
        public int cbWndExtra;
        public IntPtr hInstance;
        public IntPtr hIcon;
        public IntPtr hCursor;
        public IntPtr hbrBackground;
        public IntPtr lpszMenuName;
        public string lpszClassName;
        public IntPtr hIconSm;
    }

    internal static class Natif
    {
        public const int WM_DESTROY = 0x0002, WM_SIZE = 0x0005, WM_SETTEXT = 0x000C, WM_CLOSE = 0x0010,
            WM_SHOWWINDOW = 0x0018, WM_NCDESTROY = 0x0082, WM_USER = 0x0400;
        public const int WS_OVERLAPPED = 0, WS_POPUP = unchecked((int)0x80000000), WS_CHILD = 0x40000000,
            WS_VISIBLE = 0x10000000, WS_CLIPSIBLINGS = 0x04000000, WS_CLIPCHILDREN = 0x02000000,
            WS_CAPTION = 0x00C00000, WS_BORDER = 0x00800000, WS_DLGFRAME = 0x00400000, WS_SYSMENU = 0x00080000,
            WS_THICKFRAME = 0x00040000, WS_MINIMIZEBOX = 0x00020000, WS_MAXIMIZEBOX = 0x00010000;
        public const int WS_EX_DLGMODALFRAME = 0x1, WS_EX_TOOLWINDOW = 0x80, WS_EX_WINDOWEDGE = 0x100,
            WS_EX_CLIENTEDGE = 0x200, WS_EX_CONTROLPARENT = 0x10000;
        public const int CW_USEDEFAULT = unchecked((int)0x80000000);
        public const int SW_HIDE = 0, SW_SHOWNORMAL = 1, SW_SHOW = 5;
        public const int SWP_NOSIZE = 0x1, SWP_NOMOVE = 0x2, SWP_NOZORDER = 0x4, SWP_NOACTIVATE = 0x10;

        [DllImport("user32.dll", CharSet = CharSet.Unicode)]
        public static extern ushort RegisterClassExW(IntPtr wndclass);
        [DllImport("user32.dll", CharSet = CharSet.Unicode)]
        public static extern IntPtr CreateWindowExW(int exStyle, string className, string windowName, int style,
            int x, int y, int width, int height, IntPtr parent, IntPtr menu, IntPtr instance, IntPtr param);
        [DllImport("user32.dll")]
        public static extern IntPtr DefWindowProcW(IntPtr hwnd, int msg, IntPtr wparam, IntPtr lparam);
        [DllImport("user32.dll")]
        public static extern bool DestroyWindow(IntPtr hwnd);
        [DllImport("user32.dll")]
        public static extern bool ShowWindow(IntPtr hwnd, int cmd);
        [DllImport("user32.dll")]
        public static extern bool UpdateWindow(IntPtr hwnd);
        [DllImport("user32.dll", CharSet = CharSet.Unicode)]
        public static extern bool SetWindowTextW(IntPtr hwnd, string text);
        [DllImport("user32.dll")]
        public static extern bool SetWindowPos(IntPtr hwnd, IntPtr after, int x, int y, int cx, int cy, int flags);
        [DllImport("user32.dll")]
        public static extern bool AdjustWindowRectEx(IntPtr rect, int style, bool menu, int exStyle);
        [DllImport("user32.dll")]
        public static extern int GetSystemMetrics(int index);
        [DllImport("user32.dll")]
        public static extern int GetMessageW(IntPtr msg, IntPtr hwnd, int min, int max);
        [DllImport("user32.dll")]
        public static extern bool PeekMessageW(IntPtr msg, IntPtr hwnd, int min, int max, int remove);
        [DllImport("user32.dll")]
        public static extern bool TranslateMessage(IntPtr msg);
        [DllImport("user32.dll")]
        public static extern IntPtr DispatchMessageW(IntPtr msg);
        [DllImport("user32.dll")]
        public static extern void PostQuitMessage(int code);
        [DllImport("user32.dll")]
        public static extern bool PostMessageW(IntPtr hwnd, int msg, IntPtr wparam, IntPtr lparam);
        [DllImport("user32.dll")]
        public static extern bool GetWindowRect(IntPtr hwnd, IntPtr rect);
        [DllImport("user32.dll")]
        public static extern bool GetClientRect(IntPtr hwnd, IntPtr rect);
        [DllImport("kernel32.dll", CharSet = CharSet.Unicode)]
        public static extern IntPtr GetModuleHandleW(string name);

        public const string Classe = "WinluxWindowsForms";
        private static ProcFenetre s_proc;
        private static bool s_inscrite;

        // Une seule classe de fenêtre, une seule procédure : elle retrouve le
        // Control par sa poignée (le délégué reste vivant : champ statique).
        public static void Inscrire()
        {
            if (s_inscrite) return;
            s_proc = new ProcFenetre(Control.ProcedureCommune);
            ClasseFenetre c = new ClasseFenetre();
            c.cbSize = Marshal.SizeOf(typeof(ClasseFenetre));
            c.style = 0x3; // CS_HREDRAW | CS_VREDRAW
            c.lpfnWndProc = s_proc;
            c.hInstance = GetModuleHandleW(null);
            c.hbrBackground = new IntPtr(16); // COLOR_BTNFACE + 1
            c.lpszClassName = Classe;
            IntPtr p = Marshal.AllocHGlobal(c.cbSize);
            Marshal.StructureToPtr(c, p, false);
            RegisterClassExW(p);
            Marshal.FreeHGlobal(p);
            s_inscrite = true;
        }

        public static Rectangle Rect(IntPtr hwnd, bool client)
        {
            IntPtr r = Marshal.AllocHGlobal(16);
            if (client) GetClientRect(hwnd, r); else GetWindowRect(hwnd, r);
            int l = Marshal.ReadInt32(r, 0), t = Marshal.ReadInt32(r, 4);
            int rr = Marshal.ReadInt32(r, 8), b = Marshal.ReadInt32(r, 12);
            Marshal.FreeHGlobal(r);
            return new Rectangle(l, t, rr - l, b - t);
        }

        // Taille de fenêtre pour une zone cliente donnée (cadre et titre compris).
        public static Size FenetrePourClient(Size client, int style, int exStyle)
        {
            IntPtr r = Marshal.AllocHGlobal(16);
            Marshal.WriteInt32(r, 0, 0); Marshal.WriteInt32(r, 4, 0);
            Marshal.WriteInt32(r, 8, client.Width); Marshal.WriteInt32(r, 12, client.Height);
            AdjustWindowRectEx(r, style, false, exStyle);
            int l = Marshal.ReadInt32(r, 0), t = Marshal.ReadInt32(r, 4);
            int rr = Marshal.ReadInt32(r, 8), b = Marshal.ReadInt32(r, 12);
            Marshal.FreeHGlobal(r);
            return new Size(rr - l, b - t);
        }
    }

    // ── Application ──

    public sealed class Application
    {
        private static bool s_styles;
        private static bool s_texteCompatible;
        private static bool s_quitter;
        internal static bool s_fenetreCreee;

        private Application() { }

        public static bool RenderWithVisualStyles { get { return s_styles; } }

        // Styles visuels : nos contrôles sont toujours dessinés par le thème
        // de Winlux ; l'appel ne fait que le noter (Learn : à appeler avant
        // toute fenêtre).
        public static void EnableVisualStyles() { s_styles = true; }

        public static void SetCompatibleTextRenderingDefault(bool defaultValue)
        {
            // Learn : InvalidOperationException si une fenêtre existe déjà.
            if (s_fenetreCreee)
                throw new InvalidOperationException("SetCompatibleTextRenderingDefault must be called before the first IWin32Window object is created in the application.");
            s_texteCompatible = defaultValue;
        }

        internal static bool CompatibleTextRendering { get { return s_texteCompatible; } }

        public static void Run() { Boucle(null); }

        public static void Run(Form mainForm)
        {
            if (mainForm == null) throw new ArgumentNullException("mainForm");
            Boucle(mainForm);
        }

        private static void Boucle(Form principale)
        {
            s_quitter = false;
            if (principale != null)
            {
                principale.FormClosed += delegate { ExitThread(); };
                principale.Visible = true;
            }
            IntPtr msg = Marshal.AllocHGlobal(48);
            try
            {
                while (!s_quitter && Natif.GetMessageW(msg, IntPtr.Zero, 0, 0) > 0)
                {
                    Natif.TranslateMessage(msg);
                    Natif.DispatchMessageW(msg);
                }
            }
            finally
            {
                Marshal.FreeHGlobal(msg);
            }
        }

        // Traite les messages en attente puis rend la main (Learn : DoEvents).
        public static void DoEvents()
        {
            IntPtr msg = Marshal.AllocHGlobal(48);
            try
            {
                while (Natif.PeekMessageW(msg, IntPtr.Zero, 0, 0, 1))
                {
                    Natif.TranslateMessage(msg);
                    Natif.DispatchMessageW(msg);
                }
            }
            finally
            {
                Marshal.FreeHGlobal(msg);
            }
        }

        // Learn : Exit ferme toutes les fenêtres (FormClosing avec
        // ApplicationExitCall, annulable) puis arrête les boucles.
        public static void Exit()
        {
            foreach (Form f in Form.Ouvertes())
            {
                if (!f.FermerPour(CloseReason.ApplicationExitCall)) return;
            }
            ExitThread();
        }

        public static void ExitThread()
        {
            s_quitter = true;
            Natif.PostQuitMessage(0);
        }
    }

    // ── Control ──

    public class Control : Component, IWin32Window
    {
        private static readonly Dictionary<IntPtr, Control> s_fenetres = new Dictionary<IntPtr, Control>();
        private static Control s_enCreation;

        private IntPtr _handle;
        private string _texte = "";
        private int _x, _y, _l, _h;
        private bool _visible = true;
        private bool _actif = true;
        private bool _detruit;
        private int _gelMiseEnPage;
        private Control _parent;
        private ControlCollection _enfants;
        private string _nom = "";
        private object _tag;

        public Control() { Size d = DefaultSize; _l = d.Width; _h = d.Height; }

        public Control(string text) : this() { Text = text; }

        protected virtual Size DefaultSize { get { return Size.Empty; } }

        public event EventHandler Click;
        public event EventHandler TextChanged;
        public event EventHandler VisibleChanged;
        public event EventHandler SizeChanged;
        public event EventHandler Resize;
        public event LayoutEventHandler Layout;
        public event EventHandler HandleCreated;
        public event EventHandler HandleDestroyed;

        // ── Poignée ──

        public IntPtr Handle { get { if (_handle == IntPtr.Zero) CreateHandle(); return _handle; } }
        public bool IsHandleCreated { get { return _handle != IntPtr.Zero; } }
        public bool IsDisposed { get { return _detruit; } }
        public bool Created { get { return _handle != IntPtr.Zero; } }

        internal virtual int StyleFenetre
        {
            get
            {
                int s = Natif.WS_CLIPCHILDREN | Natif.WS_CLIPSIBLINGS;
                if (_parent != null) s |= Natif.WS_CHILD;
                return s;
            }
        }

        internal virtual int StyleEtendu { get { return 0; } }
        internal virtual string ClasseFenetre { get { return Natif.Classe; } }

        // Position à la création (CW_USEDEFAULT pour une Form sans position).
        internal virtual Rectangle BornesCreation { get { return new Rectangle(_x, _y, _l, _h); } }

        public void CreateControl() { if (_handle == IntPtr.Zero) CreateHandle(); }

        protected virtual void CreateHandle()
        {
            if (_detruit) throw new ObjectDisposedException(GetType().Name);
            if (_handle != IntPtr.Zero) return;
            Natif.Inscrire();
            Application.s_fenetreCreee = true;
            IntPtr parent = _parent != null ? _parent.Handle : IntPtr.Zero;
            Rectangle b = BornesCreation;
            s_enCreation = this;
            IntPtr h = Natif.CreateWindowExW(StyleEtendu, ClasseFenetre, _texte, StyleFenetre,
                b.X, b.Y, b.Width, b.Height, parent, IntPtr.Zero, Natif.GetModuleHandleW(null), IntPtr.Zero);
            s_enCreation = null;
            if (h == IntPtr.Zero) throw new InvalidOperationException("CreateWindowEx failed");
            if (_handle == IntPtr.Zero) Associer(h);
            // Bornes réelles (CW_USEDEFAULT, cadre ajusté par le système).
            LireBornes();
            if (_enfants != null)
                foreach (Control c in _enfants) c.CreateControl();
            OnHandleCreated(EventArgs.Empty);
        }

        private void Associer(IntPtr h)
        {
            _handle = h;
            lock (s_fenetres) s_fenetres[h] = this;
        }

        protected virtual void DestroyHandle()
        {
            if (_handle != IntPtr.Zero) Natif.DestroyWindow(_handle);
        }

        internal void LireBornes()
        {
            if (_handle == IntPtr.Zero) return;
            Rectangle r = Natif.Rect(_handle, false);
            if (_parent == null) { _x = r.X; _y = r.Y; }
            _l = r.Width; _h = r.Height;
        }

        // Procédure de fenêtre commune : retrouve le Control et lui passe le message.
        internal static IntPtr ProcedureCommune(IntPtr hwnd, int msg, IntPtr wparam, IntPtr lparam)
        {
            Control c;
            bool connu;
            lock (s_fenetres) connu = s_fenetres.TryGetValue(hwnd, out c);
            if (!connu && s_enCreation != null)
            {
                c = s_enCreation;
                c.Associer(hwnd);
                connu = true;
            }
            if (!connu) return Natif.DefWindowProcW(hwnd, msg, wparam, lparam);
            Message m = Message.Create(hwnd, msg, wparam, lparam);
            c.WndProc(ref m);
            return m.Result;
        }

        protected virtual void WndProc(ref Message m)
        {
            switch (m.Msg)
            {
                case Natif.WM_SIZE:
                    LireBornes();
                    OnSizeChanged(EventArgs.Empty);
                    OnResize(EventArgs.Empty);
                    DefWndProc(ref m);
                    break;
                case Natif.WM_DESTROY:
                    DefWndProc(ref m);
                    OnHandleDestroyed(EventArgs.Empty);
                    break;
                case Natif.WM_NCDESTROY:
                    DefWndProc(ref m);
                    lock (s_fenetres) s_fenetres.Remove(_handle);
                    _handle = IntPtr.Zero;
                    break;
                default:
                    DefWndProc(ref m);
                    break;
            }
        }

        protected void DefWndProc(ref Message m)
        {
            m.Result = Natif.DefWindowProcW(m.HWnd, m.Msg, m.WParam, m.LParam);
        }

        protected virtual void OnHandleCreated(EventArgs e) { EventHandler h = HandleCreated; if (h != null) h(this, e); }
        protected virtual void OnHandleDestroyed(EventArgs e) { EventHandler h = HandleDestroyed; if (h != null) h(this, e); }

        // ── Texte, nom, visibilité ──

        public virtual string Text
        {
            get { return _texte; }
            set
            {
                string v = value ?? "";
                if (v == _texte) return;
                _texte = v;
                if (_handle != IntPtr.Zero) Natif.SetWindowTextW(_handle, v);
                OnTextChanged(EventArgs.Empty);
            }
        }

        public string Name { get { return _nom; } set { _nom = value ?? ""; } }
        public object Tag { get { return _tag; } set { _tag = value; } }

        protected virtual void OnTextChanged(EventArgs e) { EventHandler h = TextChanged; if (h != null) h(this, e); }

        public bool Visible
        {
            get { return _visible && (_handle != IntPtr.Zero || _parent != null) && (_parent == null || _parent.Visible); }
            set { SetVisibleCore(value); }
        }

        protected virtual void SetVisibleCore(bool value)
        {
            bool avant = _visible && _handle != IntPtr.Zero;
            _visible = value;
            if (value && _handle == IntPtr.Zero && (_parent == null || _parent.IsHandleCreated)) CreateHandle();
            if (_handle != IntPtr.Zero) Natif.ShowWindow(_handle, value ? Natif.SW_SHOW : Natif.SW_HIDE);
            if (avant != (value && _handle != IntPtr.Zero)) OnVisibleChanged(EventArgs.Empty);
        }

        protected virtual void OnVisibleChanged(EventArgs e) { EventHandler h = VisibleChanged; if (h != null) h(this, e); }

        public void Show() { Visible = true; }
        public void Hide() { Visible = false; }

        public bool Enabled
        {
            get { return _actif && (_parent == null || _parent.Enabled); }
            set { _actif = value; }
        }

        // ── Géométrie ──

        public int Left { get { return _x; } set { SetBounds(value, _y, _l, _h); } }
        public int Top { get { return _y; } set { SetBounds(_x, value, _l, _h); } }
        public int Width { get { return _l; } set { SetBounds(_x, _y, value, _h); } }
        public int Height { get { return _h; } set { SetBounds(_x, _y, _l, value); } }
        public int Right { get { return _x + _l; } }
        public int Bottom { get { return _y + _h; } }
        public Point Location { get { return new Point(_x, _y); } set { SetBounds(value.X, value.Y, _l, _h); } }
        public Size Size { get { return new Size(_l, _h); } set { SetBounds(_x, _y, value.Width, value.Height); } }
        public Rectangle Bounds { get { return new Rectangle(_x, _y, _l, _h); } set { SetBounds(value.X, value.Y, value.Width, value.Height); } }

        public Size ClientSize
        {
            get
            {
                if (_handle != IntPtr.Zero) return Natif.Rect(_handle, true).Size;
                Size cadre = Natif.FenetrePourClient(Size.Empty, StyleFenetre, StyleEtendu);
                return new Size(Math.Max(0, _l - cadre.Width), Math.Max(0, _h - cadre.Height));
            }
            set { SetClientSizeCore(value.Width, value.Height); }
        }

        public Rectangle ClientRectangle { get { return new Rectangle(Point.Empty, ClientSize); } }

        protected virtual void SetClientSizeCore(int x, int y)
        {
            Size s = Natif.FenetrePourClient(new Size(x, y), StyleFenetre, StyleEtendu);
            Size = s;
        }

        public void SetBounds(int x, int y, int width, int height)
        {
            bool taille = width != _l || height != _h;
            _x = x; _y = y; _l = width; _h = height;
            if (_handle != IntPtr.Zero)
                Natif.SetWindowPos(_handle, IntPtr.Zero, x, y, width, height, Natif.SWP_NOZORDER | Natif.SWP_NOACTIVATE);
            if (taille && _handle == IntPtr.Zero)
            {
                OnSizeChanged(EventArgs.Empty);
                OnResize(EventArgs.Empty);
            }
        }

        protected virtual void OnSizeChanged(EventArgs e) { EventHandler h = SizeChanged; if (h != null) h(this, e); }

        protected virtual void OnResize(EventArgs e)
        {
            EventHandler h = Resize;
            if (h != null) h(this, e);
            PerformLayout(this, "Bounds");
        }

        // ── Mise en page ──

        public void SuspendLayout() { _gelMiseEnPage++; }
        public void ResumeLayout() { ResumeLayout(true); }

        public void ResumeLayout(bool performLayout)
        {
            if (_gelMiseEnPage > 0) _gelMiseEnPage--;
            if (_gelMiseEnPage == 0 && performLayout) PerformLayout();
        }

        public void PerformLayout() { PerformLayout(null, null); }

        public void PerformLayout(Control affectedControl, string affectedProperty)
        {
            if (_gelMiseEnPage > 0) return;
            OnLayout(new LayoutEventArgs(affectedControl, affectedProperty));
        }

        protected virtual void OnLayout(LayoutEventArgs levent)
        {
            LayoutEventHandler h = Layout;
            if (h != null) h(this, levent);
        }

        // ── Enfants ──

        public Control Parent
        {
            get { return _parent; }
            set
            {
                if (value == _parent) return;
                if (_parent != null) _parent.Controls.Remove(this);
                if (value != null) value.Controls.Add(this);
            }
        }

        internal void PoserParent(Control p) { _parent = p; }

        public ControlCollection Controls
        {
            get { if (_enfants == null) _enfants = CreateControlsInstance(); return _enfants; }
        }

        protected virtual ControlCollection CreateControlsInstance() { return new ControlCollection(this); }

        public bool HasChildren { get { return _enfants != null && _enfants.Count > 0; } }

        // ── Clic ──

        protected virtual void OnClick(EventArgs e) { EventHandler h = Click; if (h != null) h(this, e); }

        // ── Fin de vie ──

        protected override void Dispose(bool disposing)
        {
            if (!_detruit && disposing)
            {
                if (_enfants != null)
                {
                    Control[] copie = new Control[_enfants.Count];
                    _enfants.CopyTo(copie, 0);
                    foreach (Control c in copie) c.Dispose();
                }
                DestroyHandle();
                if (_parent != null) _parent.Controls.Remove(this);
            }
            _detruit = true;
            base.Dispose(disposing);
        }

        public class ControlCollection : IList, ICollection, IEnumerable
        {
            private readonly Control _proprietaire;
            private readonly List<Control> _liste = new List<Control>();

            public ControlCollection(Control owner) { _proprietaire = owner; }

            public Control Owner { get { return _proprietaire; } }
            public int Count { get { return _liste.Count; } }
            public virtual Control this[int index] { get { return _liste[index]; } }

            public virtual Control this[string key]
            {
                get
                {
                    foreach (Control c in _liste)
                        if (string.Equals(c.Name, key, StringComparison.OrdinalIgnoreCase)) return c;
                    return null;
                }
            }

            public virtual void Add(Control value)
            {
                if (value == null) return;
                if (value._parent != null && value._parent != _proprietaire) value._parent.Controls.Remove(value);
                if (_liste.Contains(value)) return;
                _liste.Add(value);
                value.PoserParent(_proprietaire);
                if (_proprietaire.IsHandleCreated) value.CreateControl();
                _proprietaire.PerformLayout(value, "Parent");
            }

            public virtual void AddRange(Control[] controls)
            {
                if (controls == null) throw new ArgumentNullException("controls");
                _proprietaire.SuspendLayout();
                foreach (Control c in controls) Add(c);
                _proprietaire.ResumeLayout(true);
            }

            public virtual void Remove(Control value)
            {
                if (value == null || !_liste.Remove(value)) return;
                value.PoserParent(null);
                _proprietaire.PerformLayout(value, "Parent");
            }

            public void RemoveAt(int index) { Remove(_liste[index]); }
            public virtual void Clear() { while (_liste.Count > 0) Remove(_liste[_liste.Count - 1]); }
            public bool Contains(Control control) { return _liste.Contains(control); }
            public int IndexOf(Control control) { return _liste.IndexOf(control); }
            public IEnumerator GetEnumerator() { return _liste.GetEnumerator(); }
            public void CopyTo(Array array, int index) { object[] dest = (object[])array; foreach (Control c in _liste) dest[index++] = c; }

            object IList.this[int index] { get { return _liste[index]; } set { throw new NotSupportedException(); } }
            int IList.Add(object value) { Add((Control)value); return IndexOf((Control)value); }
        }
    }

    public class ScrollableControl : Control
    {
        public ScrollableControl() { }
    }

    public class ContainerControl : ScrollableControl
    {
        public ContainerControl() { }
    }

    // ── Form ──

    public class Form : ContainerControl
    {
        private static readonly List<Form> s_ouvertes = new List<Form>();

        private FormBorderStyle _bordure = FormBorderStyle.Sizable;
        private FormStartPosition _depart = FormStartPosition.WindowsDefaultLocation;
        private bool _maximiser = true, _minimiser = true, _controlBox = true, _barreDesTaches = true;
        private bool _montree, _chargee, _fermeture;
        private CloseReason _raison = CloseReason.None;

        public Form() { }

        protected override Size DefaultSize { get { return new Size(300, 300); } }

        public event EventHandler Load;
        public event EventHandler Shown;
        public event FormClosingEventHandler FormClosing;
        public event FormClosedEventHandler FormClosed;

        internal static Form[] Ouvertes() { lock (s_ouvertes) return s_ouvertes.ToArray(); }

        public FormBorderStyle FormBorderStyle
        {
            get { return _bordure; }
            set { _bordure = value; MettreStyle(); }
        }

        public FormStartPosition StartPosition { get { return _depart; } set { _depart = value; } }
        public bool MaximizeBox { get { return _maximiser; } set { _maximiser = value; MettreStyle(); } }
        public bool MinimizeBox { get { return _minimiser; } set { _minimiser = value; MettreStyle(); } }
        public bool ControlBox { get { return _controlBox; } set { _controlBox = value; MettreStyle(); } }
        public bool ShowInTaskbar { get { return _barreDesTaches; } set { _barreDesTaches = value; } }

        private void MettreStyle()
        {
            // Changer de bordure après création demanderait de refaire la
            // fenêtre ; l'appli la règle dans InitializeComponent, avant.
        }

        internal override int StyleFenetre
        {
            get
            {
                int s = Natif.WS_CLIPCHILDREN | Natif.WS_CLIPSIBLINGS;
                switch (_bordure)
                {
                    case FormBorderStyle.None: s |= Natif.WS_POPUP; break;
                    case FormBorderStyle.Sizable:
                    case FormBorderStyle.SizableToolWindow:
                        s |= Natif.WS_CAPTION | Natif.WS_THICKFRAME; break;
                    default: s |= Natif.WS_CAPTION; break;
                }
                if (_bordure != FormBorderStyle.None && _controlBox)
                {
                    s |= Natif.WS_SYSMENU;
                    if (_bordure != FormBorderStyle.FixedToolWindow && _bordure != FormBorderStyle.SizableToolWindow)
                    {
                        if (_maximiser) s |= Natif.WS_MAXIMIZEBOX;
                        if (_minimiser) s |= Natif.WS_MINIMIZEBOX;
                    }
                }
                return s;
            }
        }

        internal override int StyleEtendu
        {
            get
            {
                int e = Natif.WS_EX_CONTROLPARENT | Natif.WS_EX_WINDOWEDGE;
                if (_bordure == FormBorderStyle.FixedDialog) e |= Natif.WS_EX_DLGMODALFRAME;
                if (_bordure == FormBorderStyle.Fixed3D) e |= Natif.WS_EX_CLIENTEDGE;
                if (_bordure == FormBorderStyle.FixedToolWindow || _bordure == FormBorderStyle.SizableToolWindow)
                    e |= Natif.WS_EX_TOOLWINDOW;
                return e;
            }
        }

        internal override Rectangle BornesCreation
        {
            get
            {
                Rectangle b = Bounds;
                switch (_depart)
                {
                    case FormStartPosition.CenterScreen:
                    case FormStartPosition.CenterParent:
                        int sw = Natif.GetSystemMetrics(0), sh = Natif.GetSystemMetrics(1);
                        return new Rectangle(Math.Max(0, (sw - b.Width) / 2), Math.Max(0, (sh - b.Height) / 2), b.Width, b.Height);
                    case FormStartPosition.WindowsDefaultLocation:
                        return new Rectangle(Natif.CW_USEDEFAULT, Natif.CW_USEDEFAULT, b.Width, b.Height);
                    case FormStartPosition.WindowsDefaultBounds:
                        return new Rectangle(Natif.CW_USEDEFAULT, Natif.CW_USEDEFAULT, Natif.CW_USEDEFAULT, Natif.CW_USEDEFAULT);
                    default:
                        return b;
                }
            }
        }

        protected override void CreateHandle()
        {
            base.CreateHandle();
            lock (s_ouvertes) if (!s_ouvertes.Contains(this)) s_ouvertes.Add(this);
        }

        protected override void SetVisibleCore(bool value)
        {
            if (value && !_chargee)
            {
                // Learn : Load avant le premier affichage, Shown juste après.
                CreateControl();
                _chargee = true;
                OnLoad(EventArgs.Empty);
                if (IsDisposed) return;
                PerformLayout();
            }
            base.SetVisibleCore(value);
            if (value && !_montree && IsHandleCreated)
            {
                _montree = true;
                Natif.UpdateWindow(Handle);
                OnShown(EventArgs.Empty);
            }
        }

        protected virtual void OnLoad(EventArgs e) { EventHandler h = Load; if (h != null) h(this, e); }
        protected virtual void OnShown(EventArgs e) { EventHandler h = Shown; if (h != null) h(this, e); }
        protected virtual void OnFormClosing(FormClosingEventArgs e) { FormClosingEventHandler h = FormClosing; if (h != null) h(this, e); }
        protected virtual void OnFormClosed(FormClosedEventArgs e) { FormClosedEventHandler h = FormClosed; if (h != null) h(this, e); }

        // Learn : Close envoie la fermeture (FormClosing annulable, raison
        // UserClosing), puis FormClosed et la fenêtre est détruite.
        public void Close()
        {
            if (!IsHandleCreated) { Dispose(); return; }
            Natif.PostMessageW(Handle, Natif.WM_CLOSE, IntPtr.Zero, IntPtr.Zero);
            _raison = CloseReason.UserClosing;
        }

        internal bool FermerPour(CloseReason raison)
        {
            if (_fermeture || IsDisposed) return true;
            FormClosingEventArgs e = new FormClosingEventArgs(raison, false);
            OnFormClosing(e);
            if (e.Cancel) return false;
            _fermeture = true;
            OnFormClosed(new FormClosedEventArgs(raison));
            lock (s_ouvertes) s_ouvertes.Remove(this);
            Dispose();
            return true;
        }

        protected override void WndProc(ref Message m)
        {
            if (m.Msg == Natif.WM_CLOSE)
            {
                CloseReason r = _raison == CloseReason.None ? CloseReason.UserClosing : _raison;
                _raison = CloseReason.None;
                FermerPour(r);
                m.Result = IntPtr.Zero;
                return;
            }
            base.WndProc(ref m);
        }
    }
}
