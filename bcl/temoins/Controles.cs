// Témoin : Label, Button et ProgressBar de WinForms servis par notre
// mscorlib, en vraies fenêtres enfants (#2795, suivi #2752).
// Façade de compilation : System.Windows.Forms.
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 9) de la première vérification qui échoue.
// Avec WINLUX_TEMOIN_ATTENDRE=1, la fenêtre reste ouverte jusqu'à ce qu'on
// la ferme (capture sous Xvfb).
// La fenêtre principale du téléchargeur de Paint.NET mourait sur
// Control.AutoSize de son premier Label.
extern alias Facade;
using System;
using System.Drawing;
using System.Runtime.InteropServices;
using Application = Facade::System.Windows.Forms.Application;
using Form = Facade::System.Windows.Forms.Form;
using Label = Facade::System.Windows.Forms.Label;
using Button = Facade::System.Windows.Forms.Button;
using ProgressBar = Facade::System.Windows.Forms.ProgressBar;
using Style = Facade::System.Windows.Forms.ProgressBarStyle;
using Resultat = Facade::System.Windows.Forms.DialogResult;
using Message = Facade::System.Windows.Forms.Message;
using Keys = Facade::System.Windows.Forms.Keys;

class Principale : Form
{
    public Label Titre = new Label();
    public Label Borne = new Label();
    public Button Ok = new Button();
    public Button Annuler = new Button();
    public ProgressBar Barre = new ProgressBar();
    public int Clics, Annulations, Raccourcis, MisesEnPage;

    public Principale()
    {
        SuspendLayout();
        // Comme InitializeComponent : AutoSize avant le texte.
        Titre.AutoSize = true;
        Titre.Location = new Point(12, 9);
        Titre.Text = "Téléchargement";
        Borne.MinimumSize = new Size(50, 30);
        Borne.Size = new Size(10, 10);
        Ok.Text = "OK";
        Ok.DialogResult = Resultat.OK;
        Ok.Location = new Point(12, 120);
        Ok.Click += delegate { Clics++; };
        Annuler.Text = "Annuler";
        Annuler.Location = new Point(100, 120);
        Annuler.Click += delegate { Annulations++; };
        Barre.Location = new Point(12, 60);
        Barre.Maximum = 200;
        Barre.Value = 50;
        Controls.Add(Titre);
        Controls.Add(Borne);
        Controls.Add(Ok);
        Controls.Add(Annuler);
        Controls.Add(Barre);
        AcceptButton = Ok;
        CancelButton = Annuler;
        ClientSize = new Size(320, 160);
        Text = "Témoin contrôles";
        ResumeLayout(false);
    }

    protected override bool ProcessCmdKey(ref Message msg, Keys keyData)
    {
        if (keyData == (Keys.Control | Keys.A)) { Raccourcis++; return true; }
        return base.ProcessCmdKey(ref msg, keyData);
    }

    protected override void OnLayout(Facade::System.Windows.Forms.LayoutEventArgs levent)
    {
        MisesEnPage++;
        base.OnLayout(levent);
    }
}

static class Controles
{
    [DllImport("user32.dll")] static extern IntPtr SendMessageW(IntPtr h, int m, IntPtr w, IntPtr l);
    [DllImport("user32.dll")] static extern bool PostMessageW(IntPtr h, int m, IntPtr w, IntPtr l);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll")] static extern IntPtr GetParent(IntPtr h);
    [DllImport("user32.dll")] static extern IntPtr SetFocus(IntPtr h);

    static int Verifier(Principale f)
    {
        // 1. Le Label AutoSize a pris la taille de son texte ; un texte plus
        // long l'élargit.
        Size avant = f.Titre.Size;
        if (avant.Width <= 0 || avant.Height <= 0 || avant.Height == 23 || avant != f.Titre.PreferredSize) return 1;
        f.Titre.Text = "Téléchargement de Paint.NET en cours";
        if (f.Titre.Width <= avant.Width) return 1;

        // 2. Vraies fenêtres enfants, visibles, sous la Form.
        if (GetParent(f.Ok.Handle) != f.Handle || !IsWindowVisible(f.Titre.Handle) || !IsWindowVisible(f.Barre.Handle)) return 2;

        // 3. MinimumSize borne la taille.
        if (f.Borne.Width != 50 || f.Borne.Height != 30) return 3;

        // 4. ProgressBar : valeurs passées à la fenêtre (PBM_GETPOS), bornes vérifiées.
        if ((int)SendMessageW(f.Barre.Handle, 0x0408, IntPtr.Zero, IntPtr.Zero) != 50) return 4;
        f.Barre.Value = 120;
        if ((int)SendMessageW(f.Barre.Handle, 0x0408, IntPtr.Zero, IntPtr.Zero) != 120) return 4;
        try { f.Barre.Value = 300; return 4; } catch (ArgumentOutOfRangeException) { }
        f.Barre.Maximum = 100;
        if (f.Barre.Value != 100) return 4;
        f.Barre.Style = Style.Marquee;

        // 5. Un clic sur le bouton (BM_CLICK) revient en Click, avec son DialogResult.
        SendMessageW(f.Ok.Handle, 0x00F5, IntPtr.Zero, IntPtr.Zero);
        Application.DoEvents();
        if (f.Clics != 1 || f.DialogResult != Resultat.OK) return 5;

        // 6. Entrée clique AcceptButton, Échap CancelButton.
        SetFocus(f.Titre.Handle);
        PostMessageW(f.Titre.Handle, 0x0100, new IntPtr(13), IntPtr.Zero);
        Application.DoEvents();
        if (f.Clics != 2) return 6;
        PostMessageW(f.Titre.Handle, 0x0100, new IntPtr(27), IntPtr.Zero);
        Application.DoEvents();
        if (f.Annulations != 1) return 6;

        // 7. PerformClick et taille préférée d'un bouton.
        f.Annuler.PerformClick();
        if (f.Annulations != 2 || f.Annuler.PreferredSize.Width < 75) return 7;

        // 8. ClientSize propre à Form.
        if (f.ClientSize.Width != 320 || f.ClientSize.Height != 160) return 8;

        // 9. OnLayout redéfini par l'appli passe par Form.OnLayout.
        if (f.MisesEnPage < 1) return 9;
        return 200;
    }

    static int Main()
    {
        Application.EnableVisualStyles();
        Application.SetCompatibleTextRenderingDefault(false);
        Principale f = new Principale();
        int code = 0;
        f.Shown += delegate
        {
            try { code = Verifier(f); }
            catch (Exception e) { Console.WriteLine(e.ToString()); code = 99; }
            if (Environment.GetEnvironmentVariable("WINLUX_TEMOIN_ATTENDRE") != "1") f.Close();
        };
        Application.Run(f);
        return code;
    }
}
