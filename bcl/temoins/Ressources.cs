// Témoin des ressources de manifeste (#2824).
// Ressource incorporée : salut.txt,Temoins.Ressources.salut.txt.
// Ressource incorporée : salut.txt,racine.txt.
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 6) de la première vérification qui échoue.
// Le téléchargeur de Paint.NET charge son icône par new Icon(Type, nom), qui
// lit GetManifestResourceStream(Type, nom) ; un ResourceManager passe par le
// même chemin. Le moteur ne lisait pas la table ManifestResource.
using System;
using System.IO;
using System.Reflection;
using System.Text;

namespace Temoins.Ressources
{
    static class Programme
    {
        const string Attendu = "Bonjour depuis une ressource incorporée.\n";

        static string Lire(Stream s)
        {
            MemoryStream m = new MemoryStream();
            s.CopyTo(m);
            return Encoding.UTF8.GetString(m.ToArray());
        }

        static int Main()
        {
            Assembly a = typeof(Programme).Assembly;

            // 1. Le nom complet : les octets exacts, en flux de longueur connue.
            using (Stream s = a.GetManifestResourceStream("Temoins.Ressources.salut.txt"))
            {
                if (s == null || s.Length != Encoding.UTF8.GetByteCount(Attendu)) return 1;
                if (Lire(s) != Attendu) return 1;
            }

            // 2. (Type, nom) : préfixé par l'espace de noms du type.
            using (Stream s = a.GetManifestResourceStream(typeof(Programme), "salut.txt"))
                if (s == null || Lire(s) != Attendu) return 2;

            // 3. Une ressource sans espace de noms.
            using (Stream s = a.GetManifestResourceStream("racine.txt"))
                if (s == null || Lire(s) != Attendu) return 3;

            // 4. Un nom absent, ou d'une autre casse : null.
            if (a.GetManifestResourceStream("absente.txt") != null) return 4;
            if (a.GetManifestResourceStream("RACINE.TXT") != null) return 4;

            // 5. La liste des noms.
            string[] noms = a.GetManifestResourceNames();
            if (noms.Length != 2 || Array.IndexOf(noms, "racine.txt") < 0
                || Array.IndexOf(noms, "Temoins.Ressources.salut.txt") < 0) return 5;

            // 6. Un nom nul : ArgumentNullException.
            try { a.GetManifestResourceStream((string)null); return 6; }
            catch (ArgumentNullException) { }

            return 200;
        }
    }
}
