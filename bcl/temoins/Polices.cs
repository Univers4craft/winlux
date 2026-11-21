// Témoin : System.Drawing.Font, Control.Font et la mise à l'échelle
// automatique de WinForms servis par notre mscorlib (#2801, suivi #2752).
// Façade de compilation : System.Windows.Forms.
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 9) de la première vérification qui échoue.
// La fenêtre principale du téléchargeur de Paint.NET pose sa police
// (Font(string, float, FontStyle, GraphicsUnit, byte)) et
// AutoScaleDimensions/AutoScaleMode dans son InitializeComponent.
extern alias Facade;
using System;
using System.Drawing;
using System.Runtime.InteropServices;
using Application = Facade::System.Windows.Forms.Application;
using Form = Facade::System.Windows.Forms.Form;
using Label = Facade::System.Windows.Forms.Label;
using Button = Facade::System.Windows.Forms.Button;
using Control = Facade::System.Windows.Forms.Control;
using Mode = Facade::System.Windows.Forms.AutoScaleMode;

static class Polices
{
    [DllImport("user32.dll")] static extern IntPtr SendMessageW(IntPtr h, int m, IntPtr w, IntPtr l);
    [DllImport("user32.dll")] static extern IntPtr GetDC(IntPtr h);
    [DllImport("user32.dll")] static extern int ReleaseDC(IntPtr h, IntPtr dc);
    [DllImport("gdi32.dll")] static extern int GetDeviceCaps(IntPtr dc, int i);
    [DllImport("gdi32.dll")] static extern bool DeleteObject(IntPtr o);

    static int Dpi()
    {
        IntPtr dc = GetDC(IntPtr.Zero);
        int d = GetDeviceCaps(dc, 90);
        ReleaseDC(IntPtr.Zero, dc);
        return d;
    }

    static int Main()
    {
        // 1. La police du concepteur : propriétés telles que données.
        Font f = new Font("Segoe UI", 9F, FontStyle.Regular, GraphicsUnit.Point, (byte)0);
        if (f.Size != 9F || f.Unit != GraphicsUnit.Point || f.GdiCharSet != 0 || f.Style != FontStyle.Regular
            || f.FontFamily.Name != "Segoe UI" || f.GdiVerticalFont || f.SizeInPoints != 9F || f.Name != "Segoe UI") return 1;

        // 2. Copie avec un autre style, égalité, clone, vraie poignée GDI.
        Font g = new Font(f.FontFamily, f.Size, FontStyle.Bold, f.Unit, f.GdiCharSet, f.GdiVerticalFont);
        if (!g.Bold || g.Equals(f) || !new Font(f, FontStyle.Bold).Equals(g) || !f.Equals(f.Clone())) return 2;
        IntPtr h = f.ToHfont();
        if (h == IntPtr.Zero || f.Height <= 0) return 2;
        DeleteObject(h);

        // 3. Arguments refusés (Learn) ; pixels convertis en points.
        try { new Font("Segoe UI", 0F); return 3; } catch (ArgumentException) { }
        try { new Font("Segoe UI", 9F, FontStyle.Regular, GraphicsUnit.Display); return 3; } catch (ArgumentException) { }
        int dpi = Dpi();
        if (Math.Abs(new Font("Segoe UI", 12F, GraphicsUnit.Pixel).SizeInPoints - 12F * 72F / dpi) > 0.01F) return 3;

        // 4. Police ambiante : celle du parent, sinon DefaultFont.
        Form form = new Form();
        Label hérite = new Label();
        Label propre = new Label();
        Font grande = new Font("Segoe UI", 20F);
        propre.Font = grande;
        form.Controls.Add(hérite);
        form.Controls.Add(propre);
        if (Control.DefaultFont == null || Control.DefaultFont.Size <= 0 || !hérite.Font.Equals(Control.DefaultFont)) return 4;
        int changements = 0;
        hérite.FontChanged += delegate { changements++; };
        form.Font = f;
        if (!hérite.Font.Equals(f) || !propre.Font.Equals(grande) || changements != 1) return 4;

        // 5. La police passe à la fenêtre (WM_GETFONT).
        hérite.Text = "Téléchargement";
        hérite.AutoSize = true;
        form.ClientSize = new Size(300, 200);
        IntPtr avant = IntPtr.Zero;
        int code = 0;
        form.Shown += delegate
        {
            try
            {
                avant = SendMessageW(hérite.Handle, 0x0031, IntPtr.Zero, IntPtr.Zero);
                if (avant == IntPtr.Zero) { code = 5; return; }
                // 6. Un Label AutoSize suit sa police.
                int hauteur = hérite.Height;
                hérite.Font = grande;
                if (SendMessageW(hérite.Handle, 0x0031, IntPtr.Zero, IntPtr.Zero) == avant) { code = 5; return; }
                if (hérite.Height <= hauteur) { code = 6; return; }
                code = 200;
            }
            finally { form.Close(); }
        };
        Application.Run(form);
        if (code != 200) return code == 0 ? 5 : code;

        // 7. Échelle Dpi : conçue à dpi/2, la Form et ses contrôles doublent.
        Form dpiForm = new Form();
        Button b = new Button();
        dpiForm.SuspendLayout();
        dpiForm.AutoScaleDimensions = new SizeF(dpi / 2F, dpi / 2F);
        dpiForm.AutoScaleMode = Mode.Dpi;
        dpiForm.ClientSize = new Size(100, 80);
        b.Bounds = new Rectangle(10, 10, 50, 20);
        dpiForm.Controls.Add(b);
        dpiForm.ResumeLayout(false);
        if (b.Bounds != new Rectangle(20, 20, 100, 40)) return 7;
        if (dpiForm.ClientSize != new Size(200, 160)) return 7;
        if (dpiForm.CurrentAutoScaleDimensions != new SizeF(dpi, dpi) || dpiForm.AutoScaleDimensions != new SizeF(dpi, dpi)) return 7;

        // 8. Échelle Font : des dimensions égales aux mesures ne changent rien.
        Form fontForm = new Form();
        Button c = new Button();
        fontForm.SuspendLayout();
        fontForm.Font = f;
        fontForm.AutoScaleMode = Mode.Font;
        SizeF mesure = fontForm.CurrentAutoScaleDimensions;
        fontForm.AutoScaleDimensions = mesure;
        c.Bounds = new Rectangle(5, 5, 60, 25);
        fontForm.Controls.Add(c);
        fontForm.ResumeLayout(false);
        if (mesure.Width <= 0 || mesure.Height <= mesure.Width || c.Bounds != new Rectangle(5, 5, 60, 25)) return 8;

        // 9. Conçue pour une police deux fois plus petite : facteur ≈ 2.
        fontForm.AutoScaleDimensions = new SizeF(mesure.Width / 2F, mesure.Height / 2F);
        SizeF facteur = fontForm.AutoScaleFactor;
        if (Math.Abs(facteur.Width - 2F) > 0.01F || Math.Abs(facteur.Height - 2F) > 0.01F) return 9;
        return 200;
    }
}
