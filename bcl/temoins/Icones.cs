// Témoin : System.Drawing.Icon, Form.Icon, Application.ProductVersion et
// Clipboard de WinForms servis par notre mscorlib (#2807, suivi #2752).
// Façade de compilation : System.Windows.Forms.
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 7) de la première vérification qui échoue.
// La fenêtre principale du téléchargeur de Paint.NET pose son icône, lit
// Application.ProductVersion et copie du texte avec Clipboard.SetText.
extern alias Facade;
using System;
using System.Drawing;
using System.IO;
using System.Reflection;
using Application = Facade::System.Windows.Forms.Application;
using Form = Facade::System.Windows.Forms.Form;
using Clipboard = Facade::System.Windows.Forms.Clipboard;
using Format = Facade::System.Windows.Forms.TextDataFormat;

[assembly: AssemblyInformationalVersion("5.1.12-temoin")]
[assembly: AssemblyFileVersion("5.1.12.0")]

static class Icones
{

    // Image BMP 32 bits d'une icône : BITMAPINFOHEADER (hauteur doublée),
    // pixels, masque ET.
    static byte[] Image(int n, byte bleu)
    {
        int masque = ((n + 31) / 32) * 4 * n;
        byte[] b = new byte[40 + n * n * 4 + masque];
        Ecrire(b, 0, 40); Ecrire(b, 4, n); Ecrire(b, 8, n * 2);
        b[12] = 1; b[14] = 32;
        for (int i = 0; i < n * n; i++) { b[40 + i * 4] = bleu; b[40 + i * 4 + 3] = 255; }
        return b;
    }

    static void Ecrire(byte[] b, int i, int v) { b[i] = (byte)v; b[i + 1] = (byte)(v >> 8); b[i + 2] = (byte)(v >> 16); b[i + 3] = (byte)(v >> 24); }

    static byte[] Ico()
    {
        byte[] a = Image(16, 200), c = Image(32, 100);
        byte[] ico = new byte[6 + 32 + a.Length + c.Length];
        ico[2] = 1; ico[4] = 2;
        int debut = 38;
        byte[][] images = { a, c };
        int[] tailles = { 16, 32 };
        for (int i = 0; i < 2; i++)
        {
            int e = 6 + 16 * i;
            ico[e] = (byte)tailles[i]; ico[e + 1] = (byte)tailles[i]; ico[e + 4] = 1; ico[e + 6] = 32;
            Ecrire(ico, e + 8, images[i].Length); Ecrire(ico, e + 12, debut);
            Array.Copy(images[i], 0, ico, debut, images[i].Length);
            debut += images[i].Length;
        }
        return ico;
    }

    static int Main()
    {
        byte[] ico = Ico();

        // 1. Icône lue d'un flux, à la taille par défaut (32) ou demandée.
        Icon grande = new Icon(new MemoryStream(ico));
        Icon petite = new Icon(new MemoryStream(ico), 16, 16);
        if (grande.Width != 32 || grande.Height != 32 || petite.Size != new Size(16, 16)) return 1;
        if (grande.Handle == IntPtr.Zero || petite.Handle == IntPtr.Zero || grande.Handle == petite.Handle) return 1;

        // 2. Taille absente : la plus petite plus grande ; Save rend le .ico.
        if (new Icon(grande, 20, 20).Width != 32 || new Icon(grande, 64, 64).Width != 32) return 2;
        MemoryStream copie = new MemoryStream();
        grande.Save(copie);
        if (copie.ToArray().Length != ico.Length) return 2;

        // 3. Un flux qui n'est pas une icône est refusé (Learn : ArgumentException).
        try { new Icon(new MemoryStream(new byte[] { 1, 2, 3, 4, 5, 6, 7 })); return 3; } catch (ArgumentException) { }

        // 4. Application.ProductVersion : la version d'information.
        if (Application.ProductVersion != "5.1.12-temoin") return 4;

        // 5. Form.Icon : par défaut une icône, puis celle posée, gardée une
        // fois la fenêtre créée (WM_SETICON envoyé).
        // WINLUX_TEMOIN_ATTENDRE : la fenêtre reste ouverte pour que
        // scripts/test-icone-fenetre.sh lise son _NET_WM_ICON (#2836).
        bool attendre = Environment.GetEnvironmentVariable("WINLUX_TEMOIN_ATTENDRE") == "1";
        Form f = new Form();
        f.Text = "Témoin icône";
        if (f.Icon == null || f.Icon.Handle == IntPtr.Zero) return 5;
        f.Icon = grande;
        int code = 0;
        f.Shown += delegate
        {
            code = f.Icon == grande && f.IsHandleCreated ? 200 : 5;
            if (!attendre) f.Close();
        };
        Application.Run(f);
        if (code != 200) return 5;

        // 6. Presse-papiers : le texte posé se relit.
        Clipboard.SetText("Paint.NET 5.1.12 — téléchargé");
        if (!Clipboard.ContainsText() || Clipboard.GetText() != "Paint.NET 5.1.12 — téléchargé") return 6;
        Clipboard.SetText("<b>é</b>", Format.Html);
        if (Clipboard.GetText(Format.Html) != "<b>é</b>") return 6;

        // 7. Texte nul ou vide refusé (Learn : ArgumentNullException).
        try { Clipboard.SetText(""); return 7; } catch (ArgumentNullException) { }
        try { Clipboard.SetText(null); return 7; } catch (ArgumentNullException) { }
        return 200;
    }
}
