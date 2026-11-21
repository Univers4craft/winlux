// Témoin : System.Windows.Forms.MessageBox.Show d'une appli WinForms .NET
// Framework est servi par le moteur (#2753). Sans affichage, la boîte écrit
// son texte au journal et rend son bouton par défaut, comme MessageBoxW.
// Façade de compilation : System.Windows.Forms.
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 6) de la première vérification qui échoue.
// SetupDownloader.exe (installeur web de Paint.NET 5.1.12) mourait sur
// MissingMethodException System.Windows.Forms.MessageBox::Show.
extern alias Facade;
using MessageBox = Facade::System.Windows.Forms.MessageBox;
using Boutons = Facade::System.Windows.Forms.MessageBoxButtons;
using Icone = Facade::System.Windows.Forms.MessageBoxIcon;
using Defaut = Facade::System.Windows.Forms.MessageBoxDefaultButton;
using Options = Facade::System.Windows.Forms.MessageBoxOptions;
using Resultat = Facade::System.Windows.Forms.DialogResult;

static class BoiteMessage
{
    static int Main()
    {
        if (MessageBox.Show("Bonjour") != Resultat.OK) return 1;
        if (MessageBox.Show("Texte", "Titre") != Resultat.OK) return 2;
        if (MessageBox.Show("Continuer ?", "Titre", Boutons.YesNo) != Resultat.Yes) return 3;
        if (MessageBox.Show("Enregistrer ?", "Titre", Boutons.YesNoCancel, Icone.Question, Defaut.Button2)
            != Resultat.No) return 4;
        if (MessageBox.Show(null, "Réessayer ?", "Titre", Boutons.RetryCancel, Icone.Warning, Defaut.Button2)
            != Resultat.Cancel) return 5;
        if (MessageBox.Show("Erreur", "Titre", Boutons.AbortRetryIgnore, Icone.Error, Defaut.Button3,
            Options.RightAlign, false) != Resultat.Ignore) return 6;
        return 200;
    }
}
