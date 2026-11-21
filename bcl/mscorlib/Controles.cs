// System.Windows.Forms de la mscorlib Winlux, mur 5a (#2795, suivi #2752) :
// Label, Button et ProgressBar sur les vraies classes de fenêtres de nos
// user32/comctl32 (STATIC, BUTTON, msctls_progress32), et les types qu'ils
// emploient (Keys, DialogResult, IButtonControl, ProgressBarStyle).
// Licence GPL 3.
//
// Écrit d'après la documentation publique de .NET Framework 4.x (MS Learn :
// « Label Class », « Button Class », « ButtonBase Class », « IButtonControl »,
// « ProgressBar Class », « ProgressBarStyle », « Keys Enum »,
// « DialogResult Enum ») et celle de l'API Win32 (styles BS_*, SS_*, PBS_*,
// messages BM_*, PBM_*, notification BN_CLICKED) ; aucun code de
// Microsoft, de Mono ou de Wine.
using System;
using System.Drawing;

namespace System.Windows.Forms
{
    public enum DialogResult { None = 0, OK = 1, Cancel = 2, Abort = 3, Retry = 4, Ignore = 5, Yes = 6, No = 7 }

    // Valeurs de Learn « Keys Enum » (codes de touches virtuelles et bits
    // de modificateurs).
    [Flags]
    public enum Keys
    {
        None = 0, LButton = 1, RButton = 2, Cancel = 3, MButton = 4, Back = 8, Tab = 9, LineFeed = 10, Clear = 12,
        Return = 13, Enter = 13, ShiftKey = 16, ControlKey = 17, Menu = 18, Pause = 19, Capital = 20, CapsLock = 20,
        Escape = 27, Space = 32, Prior = 33, PageUp = 33, Next = 34, PageDown = 34, End = 35, Home = 36,
        Left = 37, Up = 38, Right = 39, Down = 40, Select = 41, Print = 42, Execute = 43, Snapshot = 44,
        PrintScreen = 44, Insert = 45, Delete = 46, Help = 47,
        D0 = 48, D1 = 49, D2 = 50, D3 = 51, D4 = 52, D5 = 53, D6 = 54, D7 = 55, D8 = 56, D9 = 57,
        A = 65, B = 66, C = 67, D = 68, E = 69, F = 70, G = 71, H = 72, I = 73, J = 74, K = 75, L = 76, M = 77,
        N = 78, O = 79, P = 80, Q = 81, R = 82, S = 83, T = 84, U = 85, V = 86, W = 87, X = 88, Y = 89, Z = 90,
        LWin = 91, RWin = 92, Apps = 93,
        NumPad0 = 96, NumPad1 = 97, NumPad2 = 98, NumPad3 = 99, NumPad4 = 100, NumPad5 = 101, NumPad6 = 102,
        NumPad7 = 103, NumPad8 = 104, NumPad9 = 105, Multiply = 106, Add = 107, Separator = 108, Subtract = 109,
        Decimal = 110, Divide = 111,
        F1 = 112, F2 = 113, F3 = 114, F4 = 115, F5 = 116, F6 = 117, F7 = 118, F8 = 119, F9 = 120, F10 = 121,
        F11 = 122, F12 = 123,
        NumLock = 144, Scroll = 145, LShiftKey = 160, RShiftKey = 161, LControlKey = 162, RControlKey = 163,
        LMenu = 164, RMenu = 165,
        KeyCode = 0xFFFF, Shift = 0x10000, Control = 0x20000, Alt = 0x40000, Modifiers = unchecked((int)0xFFFF0000)
    }

    public interface IButtonControl
    {
        DialogResult DialogResult { get; set; }
        void NotifyDefault(bool value);
        void PerformClick();
    }

    // ── Label : fenêtre STATIC ──

    public class Label : Control
    {
        public Label() { }

        protected override Size DefaultSize { get { return new Size(100, 23); } }

        internal override string ClasseFenetre { get { return "STATIC"; } }

        // SS_LEFT | SS_NOPREFIX : le texte tel quel, sans « & » souligné.
        internal override int StyleFenetre { get { return base.StyleFenetre | 0x80; } }

        // Learn : Label redéfinit AutoSize ; vrai, il prend la taille de son
        // texte (coupé aux mots si MaximumSize.Width est fixée).
        public override bool AutoSize { get { return base.AutoSize; } set { base.AutoSize = value; } }

        public override Size GetPreferredSize(Size proposedSize)
        {
            int largeurMax = MaximumSize.Width;
            if (proposedSize.Width > 1 && (largeurMax == 0 || proposedSize.Width < largeurMax)) largeurMax = proposedSize.Width;
            Size t = Natif.MesurerTexte(Font, Text, largeurMax);
            return new Size(t.Width, t.Height);
        }

        protected override void OnHandleCreated(EventArgs e)
        {
            base.OnHandleCreated(e);
            AjusterTaille();
        }
    }

    // ── Boutons ──

    public class ButtonBase : Control
    {
        private bool _defaut;

        protected ButtonBase() { }

        protected override Size DefaultSize { get { return new Size(75, 23); } }

        internal override string ClasseFenetre { get { return "BUTTON"; } }

        // WS_TABSTOP ; BS_DEFPUSHBUTTON pour le bouton par défaut.
        internal override int StyleFenetre { get { return base.StyleFenetre | 0x00010000 | (_defaut ? 1 : 0); } }

        protected bool IsDefault
        {
            get { return _defaut; }
            set
            {
                _defaut = value;
                // BM_SETSTYLE : le cadre du bouton par défaut suit.
                if (IsHandleCreated)
                    Natif.SendMessageW(Handle, 0x00F4, new IntPtr(value ? 1 : 0), new IntPtr(1));
            }
        }

        public override bool AutoSize { get { return base.AutoSize; } set { base.AutoSize = value; } }

        public override Size GetPreferredSize(Size proposedSize)
        {
            Size t = Natif.MesurerTexte(Font, Text, 0);
            // Marges du thème autour du texte, jamais plus petit que la taille par défaut.
            return new Size(Math.Max(75, t.Width + 16), Math.Max(23, t.Height + 10));
        }

        internal override void CommandeRecue(int notification)
        {
            if (notification == 0) OnClick(EventArgs.Empty); // BN_CLICKED
        }
    }

    public class Button : ButtonBase, IButtonControl
    {
        private DialogResult _resultat;

        public Button() { }

        public virtual DialogResult DialogResult { get { return _resultat; } set { _resultat = value; } }

        public virtual void NotifyDefault(bool value) { IsDefault = value; }

        // Learn : comme un clic, seulement si le bouton peut le recevoir.
        public void PerformClick()
        {
            if (Visible && Enabled) OnClick(EventArgs.Empty);
        }

        // Learn : un bouton avec un DialogResult le donne à sa Form.
        protected override void OnClick(EventArgs e)
        {
            Form f = FindForm();
            if (f != null && _resultat != DialogResult.None) f.DialogResult = _resultat;
            base.OnClick(e);
        }
    }

    // ── ProgressBar : fenêtre msctls_progress32 ──

    // Learn « BoundsSpecified Enum ».
    [Flags]
    public enum BoundsSpecified { None = 0, X = 1, Y = 2, Location = 3, Width = 4, Height = 8, Size = 12, All = 15 }

    public enum ProgressBarStyle { Blocks = 0, Continuous = 1, Marquee = 2 }

    public class ProgressBar : Control
    {
        private int _min, _max = 100, _valeur, _pas = 10, _vitesse = 100;
        private ProgressBarStyle _style = ProgressBarStyle.Blocks;

        public ProgressBar() { }

        protected override Size DefaultSize { get { return new Size(100, 23); } }

        internal override string ClasseFenetre { get { return "msctls_progress32"; } }

        // PBS_SMOOTH (1) pour Continuous, PBS_MARQUEE (8) pour Marquee.
        private int StylePropre
        {
            get { return _style == ProgressBarStyle.Marquee ? 8 : _style == ProgressBarStyle.Continuous ? 1 : 0; }
        }

        internal override int StyleFenetre { get { return base.StyleFenetre | StylePropre; } }

        public int Minimum
        {
            get { return _min; }
            set
            {
                if (value < 0) throw new ArgumentOutOfRangeException("Minimum");
                _min = value;
                if (_max < value) _max = value; // Learn : le maximum suit
                if (_valeur < value) _valeur = value;
                Envoyer();
            }
        }

        public int Maximum
        {
            get { return _max; }
            set
            {
                if (value < 0) throw new ArgumentOutOfRangeException("Maximum");
                _max = value;
                if (_min > value) _min = value; // Learn : le minimum suit
                if (_valeur > value) _valeur = value;
                Envoyer();
            }
        }

        public int Value
        {
            get { return _valeur; }
            set
            {
                // Learn : ArgumentOutOfRangeException hors de [Minimum, Maximum].
                if (value < _min || value > _max) throw new ArgumentOutOfRangeException("Value");
                _valeur = value;
                Envoyer();
            }
        }

        public int Step { get { return _pas; } set { _pas = value; } }

        public int MarqueeAnimationSpeed
        {
            get { return _vitesse; }
            set
            {
                if (value < 0) throw new ArgumentOutOfRangeException("MarqueeAnimationSpeed");
                _vitesse = value;
                Envoyer();
            }
        }

        public ProgressBarStyle Style
        {
            get { return _style; }
            set
            {
                _style = value;
                if (IsHandleCreated)
                {
                    int s = Natif.GetWindowLongW(Handle, Natif.GWL_STYLE) & ~9;
                    Natif.SetWindowLongW(Handle, Natif.GWL_STYLE, s | StylePropre);
                }
                Envoyer();
            }
        }

        public void Increment(int value)
        {
            long v = (long)_valeur + value;
            _valeur = (int)Math.Max(_min, Math.Min(_max, v));
            Envoyer();
        }

        public void PerformStep() { Increment(_pas); }

        protected override void OnHandleCreated(EventArgs e)
        {
            base.OnHandleCreated(e);
            Envoyer();
        }

        private void Envoyer()
        {
            if (!IsHandleCreated) return;
            IntPtr h = Handle;
            Natif.SendMessageW(h, 0x0406, new IntPtr(_min), new IntPtr(_max)); // PBM_SETRANGE32
            Natif.SendMessageW(h, 0x0402, new IntPtr(_valeur), IntPtr.Zero);   // PBM_SETPOS
            bool defile = _style == ProgressBarStyle.Marquee && _vitesse > 0;
            Natif.SendMessageW(h, 0x040A, new IntPtr(defile ? 1 : 0), new IntPtr(_vitesse)); // PBM_SETMARQUEE
        }
    }
}
