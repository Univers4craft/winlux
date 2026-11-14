// Témoin de Hashtable, Environment.GetEnvironmentVariables et
// Console.CursorLeft dans la mscorlib Winlux (#2233).
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 5) de la première vérification qui échoue.
// L'installeur de Razer Synapse 4 (RazerInstaller.exe) appelait
// Console::get_CursorLeft et Environment::GetEnvironmentVariables pendant
// son extraction.
using System;
using System.Collections;
using System.IO;

namespace Temoins
{
    static class Dictionnaires
    {
        static int Main()
        {
            // 1. Hashtable : ajout, lecture, clé absente, doublon, null.
            Hashtable t = new Hashtable();
            t.Add("a", 1);
            t["b"] = "deux";
            t[3] = null;
            if (t.Count != 3 || (int)t["a"] != 1 || (string)t["b"] != "deux" || t["absente"] != null) return 1;
            if (!t.Contains("a") || !t.ContainsKey(3) || t.ContainsKey("c") || !t.ContainsValue("deux") || !t.ContainsValue(null)) return 1;
            try { t.Add("a", 2); return 1; } catch (ArgumentException) { }
            try { t[null] = 1; return 1; } catch (ArgumentNullException) { }
            t.Remove("a");
            if (t.Count != 2 || t.Contains("a")) return 1;

            // 2. Parcours : DictionaryEntry, Keys, Values.
            int vus = 0;
            foreach (DictionaryEntry e in t) { if (e.Key.Equals("b") && (string)e.Value == "deux") vus++; }
            IDictionaryEnumerator en = t.GetEnumerator();
            while (en.MoveNext()) { if (3.Equals(en.Key) && en.Value == null && en.Entry.Key.Equals(3)) vus++; }
            if (vus != 2 || t.Keys.Count != 2 || t.Values.Count != 2) return 2;
            int cles = 0;
            foreach (object k in t.Keys) if (k.Equals("b") || k.Equals(3)) cles++;
            if (cles != 2) return 2;
            Hashtable copie = new Hashtable(t);
            if (copie.Count != 2 || (string)copie["b"] != "deux" || ((Hashtable)t.Clone()).Count != 2) return 2;
            t.Clear();
            if (t.Count != 0) return 2;

            // 3. IDictionary vu comme une interface.
            IDictionary d = new Hashtable();
            d["x"] = "y";
            if (d.Count != 1 || (string)d["x"] != "y" || d.IsReadOnly || d.IsFixedSize) return 3;

            // 4. Les variables d'environnement du processus.
            IDictionary env = Environment.GetEnvironmentVariables();
            if (env == null || env.Count == 0) return 4;
            string chemin = Environment.GetEnvironmentVariable("PATH");
            if (chemin != null && (string)env["PATH"] != chemin) return 4;
            foreach (DictionaryEntry e in env)
            {
                if (!(e.Key is string) || !(e.Value is string)) return 4;
                if (Environment.GetEnvironmentVariable((string)e.Key) != (string)e.Value) return 4;
            }
            if (Environment.GetEnvironmentVariables(EnvironmentVariableTarget.Process).Count != env.Count) return 4;
            if (Environment.GetEnvironmentVariables(EnvironmentVariableTarget.User) == null) return 4;

            // 5. CursorLeft : une colonne avec une console, IOException sans
            //    (le témoin tourne sortie redirigée), jamais
            //    MissingMethodException.
            try
            {
                if (Console.CursorLeft < 0) return 5;
            }
            catch (IOException) { }
            return 200;
        }
    }
}
