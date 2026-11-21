// System.Drawing de la mscorlib Winlux, mur 5b (#2801, suivi #2752) : Font,
// FontFamily, FontStyle, GraphicsUnit et SystemFonts, sur de vraies polices
// GDI (CreateFontW de nos gdi32). Licence GPL 3.
//
// Écrit d'après la documentation publique de .NET Framework 4.x (MS Learn :
// « Font Class », « Font.ToHfont », « Font.SizeInPoints », « FontFamily
// Class », « FontStyle Enum », « GraphicsUnit Enum », « SystemFonts Class »)
// et celle de l'API Win32 (CreateFontW, LOGFONTW, GetDeviceCaps,
// GetTextMetricsW, GetObjectW) ; aucun code de Microsoft, de Mono ou de Wine.
using System;
using System.Runtime.InteropServices;

namespace System
{
    // Learn « ICloneable Interface » : Font l'implémente.
    public interface ICloneable
    {
        object Clone();
    }
}

namespace System.Drawing
{
    [Flags]
    public enum FontStyle { Regular = 0, Bold = 1, Italic = 2, Underline = 4, Strikeout = 8 }

    public enum GraphicsUnit { World = 0, Display = 1, Pixel = 2, Point = 3, Inch = 4, Document = 5, Millimeter = 6 }

    internal static class Gdi
    {
        [DllImport("gdi32.dll", CharSet = CharSet.Unicode)]
        public static extern IntPtr CreateFontW(int height, int width, int escapement, int orientation, int weight,
            int italic, int underline, int strikeOut, int charSet, int outPrecision, int clipPrecision, int quality,
            int pitchAndFamily, string face);
        [DllImport("gdi32.dll")]
        public static extern bool DeleteObject(IntPtr obj);
        [DllImport("gdi32.dll")]
        public static extern int GetDeviceCaps(IntPtr hdc, int index);
        [DllImport("gdi32.dll")]
        public static extern IntPtr GetStockObject(int index);
        [DllImport("gdi32.dll")]
        public static extern int GetObjectW(IntPtr obj, int size, IntPtr buffer);
        [DllImport("gdi32.dll")]
        public static extern IntPtr SelectObject(IntPtr hdc, IntPtr obj);
        [DllImport("gdi32.dll")]
        public static extern bool GetTextMetricsW(IntPtr hdc, IntPtr metrics);
        [DllImport("gdi32.dll", CharSet = CharSet.Unicode)]
        public static extern bool GetTextExtentPoint32W(IntPtr hdc, string text, int count, IntPtr size);
        [DllImport("user32.dll")]
        public static extern IntPtr GetDC(IntPtr hwnd);
        [DllImport("user32.dll")]
        public static extern int ReleaseDC(IntPtr hwnd, IntPtr hdc);

        // Points par pouce de l'écran (LOGPIXELSX 88, LOGPIXELSY 90).
        public static int Dpi(bool vertical)
        {
            IntPtr hdc = GetDC(IntPtr.Zero);
            int d = GetDeviceCaps(hdc, vertical ? 90 : 88);
            ReleaseDC(IntPtr.Zero, hdc);
            return d > 0 ? d : 96;
        }

        // Hauteur de ligne (tmHeight + tmExternalLeading) et largeur moyenne
        // des 52 lettres latines d'une police, en pixels.
        public static void Mesures(IntPtr hfont, out int hauteur, out int interligne, out float largeurMoyenne)
        {
            IntPtr hdc = GetDC(IntPtr.Zero);
            IntPtr avant = SelectObject(hdc, hfont);
            IntPtr tm = Marshal.AllocHGlobal(64);
            for (int i = 0; i < 64; i += 4) Marshal.WriteInt32(tm, i, 0);
            GetTextMetricsW(hdc, tm);
            hauteur = Marshal.ReadInt32(tm, 0);
            interligne = hauteur + Marshal.ReadInt32(tm, 16); // tmExternalLeading
            IntPtr sz = Marshal.AllocHGlobal(8);
            const string lettres = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
            Marshal.WriteInt32(sz, 0, 0);
            GetTextExtentPoint32W(hdc, lettres, lettres.Length, sz);
            largeurMoyenne = Marshal.ReadInt32(sz, 0) / 52f;
            Marshal.FreeHGlobal(sz);
            Marshal.FreeHGlobal(tm);
            SelectObject(hdc, avant);
            ReleaseDC(IntPtr.Zero, hdc);
        }
    }

    public sealed class FontFamily : IDisposable
    {
        private readonly string _nom;

        public FontFamily(string name)
        {
            if (string.IsNullOrEmpty(name)) throw new ArgumentException("Font family name cannot be empty.", "name");
            _nom = name;
        }

        public string Name { get { return _nom; } }

        public static FontFamily GenericSansSerif { get { return new FontFamily("Microsoft Sans Serif"); } }
        public static FontFamily GenericSerif { get { return new FontFamily("Times New Roman"); } }
        public static FontFamily GenericMonospace { get { return new FontFamily("Courier New"); } }

        public void Dispose() { }

        public override bool Equals(object obj)
        {
            FontFamily f = obj as FontFamily;
            return f != null && string.Equals(f._nom, _nom, StringComparison.OrdinalIgnoreCase);
        }

        public override int GetHashCode() { return _nom.ToLowerInvariant().GetHashCode(); }

        public override string ToString() { return "[FontFamily: Name=" + _nom + "]"; }
    }

    public sealed class Font : MarshalByRefObject, ICloneable, IDisposable
    {
        private readonly FontFamily _famille;
        private readonly float _taille;
        private readonly FontStyle _style;
        private readonly GraphicsUnit _unite;
        private readonly byte _jeu;
        private readonly bool _vertical;
        private IntPtr _hfont;

        public Font(string familyName, float emSize) : this(new FontFamily(familyName), emSize, FontStyle.Regular, GraphicsUnit.Point, 1, false) { }
        public Font(string familyName, float emSize, FontStyle style) : this(new FontFamily(familyName), emSize, style, GraphicsUnit.Point, 1, false) { }
        public Font(string familyName, float emSize, GraphicsUnit unit) : this(new FontFamily(familyName), emSize, FontStyle.Regular, unit, 1, false) { }
        public Font(string familyName, float emSize, FontStyle style, GraphicsUnit unit) : this(new FontFamily(familyName), emSize, style, unit, 1, false) { }
        public Font(string familyName, float emSize, FontStyle style, GraphicsUnit unit, byte gdiCharSet) : this(new FontFamily(familyName), emSize, style, unit, gdiCharSet, false) { }
        public Font(string familyName, float emSize, FontStyle style, GraphicsUnit unit, byte gdiCharSet, bool gdiVerticalFont) : this(new FontFamily(familyName), emSize, style, unit, gdiCharSet, gdiVerticalFont) { }
        public Font(FontFamily family, float emSize) : this(family, emSize, FontStyle.Regular, GraphicsUnit.Point, 1, false) { }
        public Font(FontFamily family, float emSize, FontStyle style) : this(family, emSize, style, GraphicsUnit.Point, 1, false) { }
        public Font(FontFamily family, float emSize, GraphicsUnit unit) : this(family, emSize, FontStyle.Regular, unit, 1, false) { }
        public Font(FontFamily family, float emSize, FontStyle style, GraphicsUnit unit) : this(family, emSize, style, unit, 1, false) { }
        public Font(FontFamily family, float emSize, FontStyle style, GraphicsUnit unit, byte gdiCharSet) : this(family, emSize, style, unit, gdiCharSet, false) { }

        // Learn : taille strictement positive et finie, unité autre que Display.
        public Font(FontFamily family, float emSize, FontStyle style, GraphicsUnit unit, byte gdiCharSet, bool gdiVerticalFont)
        {
            if (family == null) throw new ArgumentNullException("family");
            if (!(emSize > 0) || float.IsInfinity(emSize))
                throw new ArgumentException("Value of '" + emSize + "' is not valid for 'emSize'. 'emSize' should be greater than 0 and less than or equal to System.Single.MaxValue.", "emSize");
            if (unit == GraphicsUnit.Display) throw new ArgumentException("GraphicsUnit.Display is not a valid unit for a font.", "unit");
            _famille = family;
            _taille = emSize;
            _style = style;
            _unite = unit;
            _jeu = gdiCharSet;
            _vertical = gdiVerticalFont;
        }

        public Font(Font prototype, FontStyle newStyle)
            : this(prototype._famille, prototype._taille, newStyle, prototype._unite, prototype._jeu, prototype._vertical) { }

        public FontFamily FontFamily { get { return _famille; } }
        public string Name { get { return _famille.Name; } }
        public string OriginalFontName { get { return _famille.Name; } }
        public float Size { get { return _taille; } }
        public FontStyle Style { get { return _style; } }
        public GraphicsUnit Unit { get { return _unite; } }
        public byte GdiCharSet { get { return _jeu; } }
        public bool GdiVerticalFont { get { return _vertical; } }
        public bool Bold { get { return (_style & FontStyle.Bold) != 0; } }
        public bool Italic { get { return (_style & FontStyle.Italic) != 0; } }
        public bool Underline { get { return (_style & FontStyle.Underline) != 0; } }
        public bool Strikeout { get { return (_style & FontStyle.Strikeout) != 0; } }
        public bool IsSystemFont { get { return false; } }

        public float SizeInPoints
        {
            get
            {
                switch (_unite)
                {
                    case GraphicsUnit.Point: return _taille;
                    case GraphicsUnit.Inch: return _taille * 72f;
                    case GraphicsUnit.Document: return _taille * 72f / 300f;
                    case GraphicsUnit.Millimeter: return _taille * 72f / 25.4f;
                    default: return _taille * 72f / Gdi.Dpi(true); // Pixel, World
                }
            }
        }

        // Taille du caractère en pixels de l'écran.
        private int PixelsEm { get { return (int)Math.Round(SizeInPoints * Gdi.Dpi(true) / 72f); } }

        // Learn : interligne en pixels.
        public int Height
        {
            get
            {
                int h, interligne; float l;
                Gdi.Mesures(Hfont, out h, out interligne, out l);
                return interligne;
            }
        }

        public float GetHeight() { return Height; }

        // Police GDI propre à cet objet, créée à la première demande.
        internal IntPtr Hfont
        {
            get
            {
                if (_hfont == IntPtr.Zero) _hfont = Creer();
                return _hfont;
            }
        }

        private IntPtr Creer()
        {
            string face = _vertical ? "@" + _famille.Name : _famille.Name;
            return Gdi.CreateFontW(-PixelsEm, 0, 0, 0, Bold ? 700 : 400, Italic ? 1 : 0, Underline ? 1 : 0,
                Strikeout ? 1 : 0, _jeu, 0, 0, 0, 0, face);
        }

        // Learn : une nouvelle poignée, que l'appelant libère par DeleteObject.
        public IntPtr ToHfont() { return Creer(); }

        public object Clone() { return new Font(_famille, _taille, _style, _unite, _jeu, _vertical); }

        public void Dispose()
        {
            if (_hfont != IntPtr.Zero) Gdi.DeleteObject(_hfont);
            _hfont = IntPtr.Zero;
        }

        public override bool Equals(object obj)
        {
            Font f = obj as Font;
            return f != null && f._famille.Equals(_famille) && f._taille == _taille && f._style == _style
                && f._unite == _unite && f._jeu == _jeu && f._vertical == _vertical;
        }

        public override int GetHashCode() { return _famille.GetHashCode() ^ _taille.GetHashCode() ^ (int)_style; }

        public override string ToString()
        {
            return "[Font: Name=" + Name + ", Size=" + _taille + ", Units=" + (int)_unite + ", GdiCharSet=" + _jeu
                + ", GdiVerticalFont=" + (_vertical ? "True" : "False") + "]";
        }

        // Police d'un LOGFONTW (stock DEFAULT_GUI_FONT…).
        internal static Font DepuisGdi(IntPtr hfont, Font defaut)
        {
            IntPtr lf = Marshal.AllocHGlobal(92);
            try
            {
                if (hfont == IntPtr.Zero || Gdi.GetObjectW(hfont, 92, lf) < 92) return defaut;
                int hauteur = Marshal.ReadInt32(lf, 0);
                int poids = Marshal.ReadInt32(lf, 16);
                FontStyle s = FontStyle.Regular;
                if (poids >= 700) s |= FontStyle.Bold;
                if (Marshal.ReadByte(lf, 20) != 0) s |= FontStyle.Italic;
                string face = Marshal.PtrToStringUni(new IntPtr(lf.ToInt64() + 28));
                if (string.IsNullOrEmpty(face) || hauteur == 0) return defaut;
                float points = Math.Abs(hauteur) * 72f / Gdi.Dpi(true);
                return new Font(new FontFamily(face), points, s, GraphicsUnit.Point, Marshal.ReadByte(lf, 23), false);
            }
            finally
            {
                Marshal.FreeHGlobal(lf);
            }
        }
    }

    public sealed class SystemFonts
    {
        private static Font s_defaut;

        private SystemFonts() { }

        // Learn : la police d'interface par défaut (DEFAULT_GUI_FONT).
        public static Font DefaultFont
        {
            get
            {
                if (s_defaut == null)
                    s_defaut = Font.DepuisGdi(Gdi.GetStockObject(17), new Font("Microsoft Sans Serif", 8.25f));
                return s_defaut;
            }
        }

        public static Font MessageBoxFont { get { return DefaultFont; } }
        public static Font DialogFont { get { return DefaultFont; } }
    }
}
