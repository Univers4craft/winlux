// Témoin de System.Drawing.Icon(Type, nom) (#2838).
// Ressource incorporée : temoin.ico,Temoins.Fenetre.app.ico.
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 4) de la première vérification qui échoue.
// La fenêtre principale du téléchargeur de Paint.NET charge son icône par
// new Icon(typeof(MainForm), "….ico") ; temoin.ico porte une image 16 et
// une 32 px.
using System;
using System.Drawing;

namespace Temoins.Fenetre
{
    static class IconeRessource
    {
        static int Main()
        {
            // 1. La ressource de l'espace de noms du type, à 32 px par défaut.
            Icon i = new Icon(typeof(IconeRessource), "app.ico");
            if (i.Handle == IntPtr.Zero || i.Width != 32 || i.Height != 32) return 1;

            // 2. Ses données servent une autre taille de l'icône.
            if (new Icon(i, 16, 16).Width != 16) return 2;

            // 3. Une ressource absente : ArgumentException.
            try { new Icon(typeof(IconeRessource), "absente.ico"); return 3; }
            catch (ArgumentException e) { if (e is ArgumentNullException) return 3; }

            // 4. Type ou nom nul : ArgumentNullException.
            try { new Icon((Type)null, "app.ico"); return 4; } catch (ArgumentNullException) { }
            try { new Icon(typeof(IconeRessource), null); return 4; } catch (ArgumentNullException) { }
            return 200;
        }
    }
}
