// Témoin de Process.MainWindowHandle et MainWindowTitle (#3285).
// Façade de compilation : System.Windows.Forms.
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 4) de la première vérification qui échoue.
// Avant #3285, notre mscorlib rendait toujours IntPtr.Zero et "".
extern alias Facade;
using System;
using System.Diagnostics;
using System.Drawing;
using Application = Facade::System.Windows.Forms.Application;
using Form = Facade::System.Windows.Forms.Form;

static class FenetrePrincipale
{
    static int Main()
    {
        // 1. Sans fenêtre, pas de fenêtre principale.
        Process moi = Process.GetCurrentProcess();
        if (moi.MainWindowHandle != IntPtr.Zero || moi.MainWindowTitle != "") return 1;

        Form f = new Form();
        f.Text = "Principale 3285";
        f.ClientSize = new Size(200, 100);
        IntPtr vu = IntPtr.Zero;
        string titre = null;
        f.Shown += delegate
        {
            Process p = Process.GetCurrentProcess();
            vu = p.MainWindowHandle;
            titre = p.MainWindowTitle;
            f.Close();
        };
        Application.Run(f);
        // 2. La fenêtre montrée est la fenêtre principale, 3. avec son titre.
        if (vu == IntPtr.Zero) return 2;
        if (titre != "Principale 3285") return 3;
        // 4. Fermée, il n'y en a plus.
        if (Process.GetCurrentProcess().MainWindowHandle != IntPtr.Zero) return 4;
        return 200;
    }
}
