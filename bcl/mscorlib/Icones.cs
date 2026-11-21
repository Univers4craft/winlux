// System.Drawing.Icon et System.Windows.Forms.Clipboard de la mscorlib
// Winlux, mur 5c (#2807, suivi #2752) : icônes .ico en vraies HICON (nos
// user32), texte du presse-papiers. Licence GPL 3.
//
// Écrit d'après la documentation publique de .NET Framework 4.x (MS Learn :
// « Icon Class », « Icon(Stream, Int32, Int32) », « Clipboard Class »,
// « Clipboard.SetText », « TextDataFormat Enum ») et celle de l'API Win32
// (format ICO : ICONDIR/ICONDIRENTRY, CreateIconFromResourceEx, DestroyIcon,
// OpenClipboard, SetClipboardData, CF_UNICODETEXT) ; aucun code de
// Microsoft, de Mono ou de Wine.
//
// Pas encore : Icon(Type, string), qui lit une ressource de manifeste.
using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;

namespace System.Drawing
{
    public sealed class Icon : MarshalByRefObject, ICloneable, IDisposable
    {
        [DllImport("user32.dll")]
        private static extern IntPtr CreateIconFromResourceEx(IntPtr bits, int size, bool icon, int version, int cx, int cy, int flags);
        [DllImport("user32.dll")]
        private static extern bool DestroyIcon(IntPtr icon);

        private readonly byte[] _ico;
        private IntPtr _handle;
        private readonly bool _proprietaire;
        private readonly int _l, _h;

        public Icon(Stream stream) : this(stream, 32, 32) { }
        public Icon(Stream stream, Size size) : this(stream, size.Width, size.Height) { }
        public Icon(Stream stream, int width, int height) : this(Lire(stream), width, height) { }
        public Icon(string fileName) : this(fileName, 32, 32) { }
        public Icon(string fileName, Size size) : this(fileName, size.Width, size.Height) { }
        public Icon(string fileName, int width, int height) : this(File.ReadAllBytes(fileName), width, height) { }
        public Icon(Icon original, Size size) : this(original, size.Width, size.Height) { }

        // Learn « Icon(Type, String) » : la ressource incorporée de l'assembly
        // du type, nommée dans son espace de noms (#2838).
        public Icon(Type type, string resource) : this(Ressource(type, resource), 32, 32) { }

        private static byte[] Ressource(Type type, string resource)
        {
            if ((object)type == null) throw new ArgumentNullException("type");
            if (resource == null) throw new ArgumentNullException("resource");
            Stream s = type.Assembly.GetManifestResourceStream(type, resource);
            if (s == null)
                throw new ArgumentException("Resource '" + resource + "' cannot be found in class '" + type.ToString() + "'.");
            using (s) return Lire(s);
        }

        public Icon(Icon original, int width, int height)
        {
            if (original == null) throw new ArgumentNullException("original");
            if (original._ico == null) throw new ArgumentException("The icon has no image data.", "original");
            _ico = original._ico;
            _proprietaire = true;
            Charger(width, height, out _handle, out _l, out _h);
        }

        private Icon(byte[] ico, int width, int height)
        {
            _ico = ico;
            _proprietaire = true;
            Charger(width, height, out _handle, out _l, out _h);
        }

        private Icon(IntPtr handle, int width, int height)
        {
            _handle = handle;
            _l = width;
            _h = height;
        }

        private static byte[] Lire(Stream stream)
        {
            if (stream == null) throw new ArgumentNullException("stream");
            MemoryStream m = new MemoryStream();
            byte[] tampon = new byte[4096];
            int n;
            while ((n = stream.Read(tampon, 0, tampon.Length)) > 0) m.Write(tampon, 0, n);
            return m.ToArray();
        }

        private static int Mot(byte[] b, int i) { return b[i] | (b[i + 1] << 8); }
        private static int Long(byte[] b, int i) { return b[i] | (b[i + 1] << 8) | (b[i + 2] << 16) | (b[i + 3] << 24); }

        private static ArgumentException Invalide()
        {
            return new ArgumentException("Argument 'picture' must be a picture that can be used as a Icon.");
        }

        // ICONDIR puis ICONDIRENTRY (16 octets) : l'image de la taille
        // demandée, sinon la plus petite plus grande, sinon la plus grande ;
        // à taille égale, la plus profonde.
        private void Charger(int largeur, int hauteur, out IntPtr handle, out int l, out int h)
        {
            byte[] b = _ico;
            if (b.Length < 6 || Mot(b, 0) != 0 || Mot(b, 2) != 1 || Mot(b, 4) == 0) throw Invalide();
            int n = Mot(b, 4);
            if (b.Length < 6 + 16 * n) throw Invalide();
            int choix = -1, cl = 0, ch = 0, cp = 0;
            for (int i = 0; i < n; i++)
            {
                int e = 6 + 16 * i;
                int el = b[e] == 0 ? 256 : b[e], eh = b[e + 1] == 0 ? 256 : b[e + 1], ep = Mot(b, e + 6);
                if (choix < 0 || Meilleur(el, eh, ep, cl, ch, cp, largeur, hauteur))
                {
                    choix = i; cl = el; ch = eh; cp = ep;
                }
            }
            int entree = 6 + 16 * choix;
            int taille = Long(b, entree + 8), debut = Long(b, entree + 12);
            if (debut < 0 || taille <= 0 || debut + taille > b.Length) throw Invalide();
            IntPtr bits = Marshal.AllocHGlobal(taille);
            try
            {
                Marshal.Copy(b, debut, bits, taille);
                handle = CreateIconFromResourceEx(bits, taille, true, 0x00030000, cl, ch, 0);
            }
            finally
            {
                Marshal.FreeHGlobal(bits);
            }
            if (handle == IntPtr.Zero) throw Invalide();
            l = cl;
            h = ch;
        }

        private static bool Meilleur(int l, int h, int p, int cl, int ch, int cp, int vl, int vh)
        {
            bool exact = l == vl && h == vh, cexact = cl == vl && ch == vh;
            if (exact != cexact) return exact;
            if (exact) return p > cp;
            bool assez = l >= vl && h >= vh, cassez = cl >= vl && ch >= vh;
            if (assez != cassez) return assez;
            if (l * h != cl * ch) return assez ? l * h < cl * ch : l * h > cl * ch;
            return p > cp;
        }

        public IntPtr Handle
        {
            get
            {
                if (_handle == IntPtr.Zero) throw new ObjectDisposedException("Icon");
                return _handle;
            }
        }

        public int Width { get { return _l; } }
        public int Height { get { return _h; } }
        public Size Size { get { return new Size(_l, _h); } }

        // Learn : l'icône ne possède pas la poignée donnée.
        public static Icon FromHandle(IntPtr handle)
        {
            if (handle == IntPtr.Zero) throw new ArgumentException("Win32 handle that was passed to Icon is not valid or is the wrong type.", "handle");
            return new Icon(handle, 32, 32);
        }

        public void Save(Stream outputStream)
        {
            if (outputStream == null) throw new ArgumentNullException("outputStream");
            if (_ico == null) throw new InvalidOperationException("The icon has no image data.");
            outputStream.Write(_ico, 0, _ico.Length);
        }

        public object Clone()
        {
            return _ico != null ? new Icon(this, _l, _h) : FromHandle(_handle);
        }

        public void Dispose()
        {
            if (_proprietaire && _handle != IntPtr.Zero) DestroyIcon(_handle);
            _handle = IntPtr.Zero;
        }

        public override string ToString() { return "(Icon)"; }
    }
}

namespace System.ComponentModel
{
    // Learn « InvalidEnumArgumentException Class ».
    public class InvalidEnumArgumentException : ArgumentException
    {
        public InvalidEnumArgumentException() { }
        public InvalidEnumArgumentException(string message) : base(message) { }

        public InvalidEnumArgumentException(string argumentName, int invalidValue, Type enumClass)
            : base("The value of argument '" + argumentName + "' (" + invalidValue + ") is invalid for Enum type '"
                + (enumClass == null ? "" : enumClass.Name) + "'.", argumentName) { }
    }
}

namespace System.Windows.Forms
{
    public enum TextDataFormat { Text = 0, UnicodeText = 1, Rtf = 2, Html = 3, CommaSeparatedValue = 4 }

    public sealed class Clipboard
    {
        [DllImport("user32.dll")] private static extern bool OpenClipboard(IntPtr owner);
        [DllImport("user32.dll")] private static extern bool CloseClipboard();
        [DllImport("user32.dll")] private static extern bool EmptyClipboard();
        [DllImport("user32.dll")] private static extern IntPtr SetClipboardData(int format, IntPtr mem);
        [DllImport("user32.dll")] private static extern IntPtr GetClipboardData(int format);
        [DllImport("user32.dll")] private static extern bool IsClipboardFormatAvailable(int format);
        [DllImport("user32.dll", CharSet = CharSet.Unicode)] private static extern int RegisterClipboardFormatW(string name);
        [DllImport("kernel32.dll")] private static extern IntPtr GlobalAlloc(int flags, IntPtr bytes);
        [DllImport("kernel32.dll")] private static extern IntPtr GlobalLock(IntPtr mem);
        [DllImport("kernel32.dll")] private static extern bool GlobalUnlock(IntPtr mem);
        [DllImport("kernel32.dll")] private static extern IntPtr GlobalFree(IntPtr mem);
        [DllImport("kernel32.dll")] private static extern void Sleep(int ms);

        private const int CF_UNICODETEXT = 13;

        private Clipboard() { }

        // Format du presse-papiers ; Rtf, Html et Csv sont des formats inscrits
        // (« Rich Text Format », « HTML Format », « Csv »), en UTF-8.
        private static int Format(TextDataFormat f)
        {
            switch (f)
            {
                case TextDataFormat.Text:
                case TextDataFormat.UnicodeText: return CF_UNICODETEXT;
                case TextDataFormat.Rtf: return RegisterClipboardFormatW("Rich Text Format");
                case TextDataFormat.Html: return RegisterClipboardFormatW("HTML Format");
                case TextDataFormat.CommaSeparatedValue: return RegisterClipboardFormatW("Csv");
                default: throw new System.ComponentModel.InvalidEnumArgumentException("format", (int)f, typeof(TextDataFormat));
            }
        }

        // Learn : le presse-papiers peut être tenu par un autre programme ;
        // on réessaie dix fois à 100 ms d'écart, puis ExternalException.
        private static void Ouvrir()
        {
            for (int i = 0; i < 10; i++)
            {
                if (OpenClipboard(IntPtr.Zero)) return;
                Sleep(100);
            }
            throw new ExternalException("Requested Clipboard operation did not succeed.");
        }

        public static void SetText(string text) { SetText(text, TextDataFormat.UnicodeText); }

        public static void SetText(string text, TextDataFormat format)
        {
            if (string.IsNullOrEmpty(text)) throw new ArgumentNullException("text");
            int cf = Format(format);
            byte[] octets = cf == CF_UNICODETEXT ? Encoding.Unicode.GetBytes(text + "\0") : Encoding.UTF8.GetBytes(text + "\0");
            IntPtr mem = GlobalAlloc(0x0002, new IntPtr(octets.Length)); // GMEM_MOVEABLE
            if (mem == IntPtr.Zero) throw new ExternalException("Requested Clipboard operation did not succeed.");
            IntPtr p = GlobalLock(mem);
            Marshal.Copy(octets, 0, p, octets.Length);
            GlobalUnlock(mem);
            Ouvrir();
            try
            {
                EmptyClipboard();
                // Le système possède la mémoire une fois SetClipboardData réussi.
                if (SetClipboardData(cf, mem) == IntPtr.Zero)
                {
                    GlobalFree(mem);
                    throw new ExternalException("Requested Clipboard operation did not succeed.");
                }
            }
            finally
            {
                CloseClipboard();
            }
        }

        public static bool ContainsText() { return ContainsText(TextDataFormat.UnicodeText); }

        public static bool ContainsText(TextDataFormat format) { return IsClipboardFormatAvailable(Format(format)); }

        public static string GetText() { return GetText(TextDataFormat.UnicodeText); }

        // Learn : Empty quand le presse-papiers n'a pas ce format.
        public static string GetText(TextDataFormat format)
        {
            int cf = Format(format);
            Ouvrir();
            try
            {
                IntPtr mem = GetClipboardData(cf);
                if (mem == IntPtr.Zero) return string.Empty;
                IntPtr p = GlobalLock(mem);
                if (p == IntPtr.Zero) return string.Empty;
                try
                {
                    if (cf == CF_UNICODETEXT) return Marshal.PtrToStringUni(p) ?? string.Empty;
                    int n = 0;
                    while (Marshal.ReadByte(p, n) != 0) n++;
                    byte[] b = new byte[n];
                    Marshal.Copy(p, b, 0, n);
                    return Encoding.UTF8.GetString(b);
                }
                finally
                {
                    GlobalUnlock(mem);
                }
            }
            finally
            {
                CloseClipboard();
            }
        }

        public static void Clear()
        {
            Ouvrir();
            try { EmptyClipboard(); }
            finally { CloseClipboard(); }
        }
    }
}
