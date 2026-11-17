// Témoin : Application et Form de base de WinForms servis par notre
// mscorlib, sur une vraie fenêtre Win32 (#2764, suivi #2752).
// Façade de compilation : System.Windows.Forms.
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 9) de la première vérification qui échoue.
// Avec WINLUX_TEMOIN_ATTENDRE=1, la fenêtre reste ouverte jusqu'à ce qu'on
// la ferme (test Xvfb scripts/test-fenetre-winforms.sh).
// SetupDownloader.exe (installeur web de Paint.NET 5.1.12) mourait sur
// Application::EnableVisualStyles, puis demande Application.Run et une Form
// dérivée qui redéfinit OnLayout et OnFormClosing.
extern alias Facade;
using System;
using System.Drawing;
using Application = Facade::System.Windows.Forms.Application;
using Form = Facade::System.Windows.Forms.Form;
using Bordure = Facade::System.Windows.Forms.FormBorderStyle;
using Depart = Facade::System.Windows.Forms.FormStartPosition;
using Raison = Facade::System.Windows.Forms.CloseReason;
using Fermeture = Facade::System.Windows.Forms.FormClosingEventArgs;
using MiseEnPage = Facade::System.Windows.Forms.LayoutEventArgs;

class MaFenetre : Form
{
    public int MisesEnPage;
    public int Fermetures;
    public Raison Pourquoi;

    public MaFenetre()
    {
        SuspendLayout();
        Text = "Témoin WinForms";
        ClientSize = new Size(320, 200);
        FormBorderStyle = Bordure.FixedDialog;
        StartPosition = Depart.CenterScreen;
        MaximizeBox = false;
        ResumeLayout(false);
    }

    protected override void OnLayout(MiseEnPage levent) { MisesEnPage++; base.OnLayout(levent); }

    protected override void OnFormClosing(Fermeture e)
    {
        Fermetures++;
        Pourquoi = e.CloseReason;
        base.OnFormClosing(e);
    }
}

static class FenetreBase
{
    static int Main()
    {
        Application.EnableVisualStyles();
        Application.SetCompatibleTextRenderingDefault(false);
        MaFenetre f = new MaFenetre();
        if (f.Text != "Témoin WinForms") return 1;
        bool attendre = Environment.GetEnvironmentVariable("WINLUX_TEMOIN_ATTENDRE") == "1";
        int montree = 0;
        bool creee = false, visible = false;
        Size client = Size.Empty;
        f.Shown += delegate
        {
            montree++;
            creee = f.IsHandleCreated && f.Handle != IntPtr.Zero;
            visible = f.Visible;
            client = f.ClientSize;
            if (!attendre) f.Close();
        };
        Application.Run(f);
        if (montree != 1) return 2;
        if (!creee || !visible) return 3;
        if (client.Width != 320 || client.Height != 200) return 4;
        if (f.Fermetures != 1 || f.Pourquoi != Raison.UserClosing) return 5;
        if (f.MisesEnPage < 1) return 6;
        if (!f.IsDisposed) return 7;
        // Learn : trop tard une fois une fenêtre créée.
        try { Application.SetCompatibleTextRenderingDefault(true); return 8; }
        catch (InvalidOperationException) { }
        if (f.IsHandleCreated) return 9;
        return 200;
    }
}
