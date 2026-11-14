// Témoin de System.ComponentModel.Component et Container dans la mscorlib
// Winlux (#2305).
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 5) de la première vérification qui échoue.
// Corsair.Service.exe (iCUE 4.18) mourait sur Component::Dispose.
using System;
using System.ComponentModel;

namespace Temoins
{
    class Service : Component
    {
        public int Liberations;
        public bool DernierDisposing;
        protected override void Dispose(bool disposing)
        {
            Liberations++;
            DernierDisposing = disposing;
            base.Dispose(disposing);
        }
    }

    static class Composants
    {
        static int Main()
        {
            // 1. Dispose() appelle Dispose(true) puis lève Disposed.
            Service s = new Service();
            int vus = 0;
            object emetteur = null;
            s.Disposed += delegate(object o, EventArgs e) { vus++; emetteur = o; };
            s.Dispose();
            if (s.Liberations != 1 || !s.DernierDisposing || vus != 1 || emetteur != s) return 1;

            // 2. Sans site : pas de conteneur, ToString rend le nom du type.
            if (s.Site != null || s.Container != null || s.ToString() != "Temoins.Service") return 2;

            // 3. Container.Add donne un site nommé ; ToString le reprend.
            Container c = new Container();
            Service a = new Service();
            Service b = new Service();
            c.Add(a, "premier");
            c.Add(b);
            if (a.Site == null || a.Site.Name != "premier" || a.Container != c || c.Components.Count != 2) return 3;
            if (a.ToString() != "premier [Temoins.Service]" || c.Components["PREMIER"] != a) return 3;
            try { c.Add(new Service(), "Premier"); return 3; } catch (ArgumentException) { }

            // 4. Dispose d'un composant le retire de son conteneur.
            a.Dispose();
            if (a.Site != null || c.Components.Count != 1 || c.Components[0] != b) return 4;

            // 5. Dispose du conteneur libère ce qu'il contient.
            c.Dispose();
            if (b.Liberations != 1 || b.Site != null || c.Components.Count != 0) return 5;
            IComponent ic = b;
            IDisposable id = ic;
            if (id == null) return 5;
            return 200;
        }
    }
}
