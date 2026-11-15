// Témoin de la réflexion sur les membres dans la mscorlib Winlux (#2008) :
// GetMethod/Invoke, propriétés, champs, constructeurs, Activator, attributs
// personnalisés, énumérations, types génériques et interfaces.
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 14) de la première vérification qui échoue.
// La 13 forge une poignée de membre par réflexion : elle doit lever
// ArgumentException au lieu de faire paniquer le moteur.
// La 14 passe à Invoke des arguments qui n'ont pas le type du paramètre
// (int[] pour string, string pour int…) : ArgumentException, non
// enveloppée, avant l'appel ; une cible d'un autre type donne
// TargetException, et FieldInfo.GetValue/SetValue refusent de même (#2037).
// Corsair.Service.exe (iCUE 4.18) appelait Type::GetMethod (11 fois),
// MemberInfo::GetCustomAttributes, MethodBase::Invoke, PropertyInfo,
// Activator::CreateInstance (10) et Enum::GetValues, Parse, ToObject.
using System;
using System.Collections.Generic;
using System.Reflection;

[assembly: AssemblyProduct("Témoin Winlux")]

namespace Temoins
{
    [AttributeUsage(AttributeTargets.All, AllowMultiple = true)]
    class MarqueAttribute : Attribute
    {
        public string Texte;
        public int Niveau { get; set; }
        public Type Cible;
        public MarqueAttribute(string texte) { Texte = texte; }
    }

    [Flags]
    enum Couleur : byte { Aucune = 0, Rouge = 1, Vert = 2, Bleu = 4 }

    interface IAppareil { int Leds { get; } }

    [Marque("clavier", Niveau = 3, Cible = typeof(int))]
    class Clavier : IAppareil
    {
        public static int Compteur = 0;
        public string Nom = "K70";
        private int _leds;
        public Clavier() { _leds = 104; }
        public Clavier(int leds) { _leds = leds; }
        public int Leds { get { return _leds; } }
        public string Modele { get; set; }
        [Marque("méthode")]
        public int Ajouter(int a, int b) { return a + b; }
        public static string Saluer(string qui) { return "bonjour " + qui; }
        public static int Taille(byte[] octets) { return octets.Length; }
        public static int Allumees(IAppareil a) { return a.Leds; }
        public static long Doubler(long v) { return 2 * v; }
        public virtual string Genre() { return "clavier"; }
        public bool Essayer(string s, out int n) { n = s.Length; return true; }
        public void Echouer() { throw new InvalidOperationException("raté"); }
        public Couleur Teinte = Couleur.Vert;
    }

    class ClavierSansFil : Clavier
    {
        public override string Genre() { return "sans fil"; }
    }

    static class Membres
    {
        static int Main()
        {
            Type t = typeof(Clavier);
            MethodInfo ajouter = t.GetMethod("Ajouter");
            if (ajouter == null || ajouter.ReturnType != typeof(int) || ajouter.GetParameters().Length != 2 || ajouter.GetParameters()[1].Name != "b") return 1;
            Clavier c = new Clavier();
            if ((int)ajouter.Invoke(c, new object[] { 2, 3 }) != 5) return 2;
            if ((string)t.GetMethod("Saluer").Invoke(null, new object[] { "iCUE" }) != "bonjour iCUE") return 2;
            // Appel virtuel : la redéfinition du type réel.
            if ((string)t.GetMethod("Genre").Invoke(new ClavierSansFil(), new object[0]) != "sans fil") return 2;
            object[] sortie = { "abcd", 0 };
            if (!(bool)t.GetMethod("Essayer").Invoke(c, sortie) || (int)sortie[1] != 4) return 3;
            try
            {
                t.GetMethod("Echouer").Invoke(c, null);
                return 3;
            }
            catch (TargetInvocationException e)
            {
                if (!(e.InnerException is InvalidOperationException)) return 3;
            }

            PropertyInfo leds = t.GetProperty("Leds");
            PropertyInfo modele = t.GetProperty("Modele");
            if (leds == null || !leds.CanRead || leds.CanWrite || leds.PropertyType != typeof(int) || (int)leds.GetValue(c, null) != 104) return 4;
            modele.SetValue(c, "K95", null);
            if (c.Modele != "K95" || (string)modele.GetValue(c) != "K95") return 4;

            FieldInfo nom = t.GetField("Nom");
            FieldInfo compteur = t.GetField("Compteur", BindingFlags.Public | BindingFlags.Static);
            nom.SetValue(c, "K100");
            compteur.SetValue(null, 7);
            if (c.Nom != "K100" || Clavier.Compteur != 7 || (int)compteur.GetValue(null) != 7) return 5;
            if (t.GetField("_leds") != null || t.GetField("_leds", BindingFlags.NonPublic | BindingFlags.Instance) == null) return 5;
            if ((Couleur)t.GetField("Teinte").GetValue(c) != Couleur.Vert) return 5;

            ConstructorInfo ctor = t.GetConstructor(new[] { typeof(int) });
            Clavier c2 = (Clavier)ctor.Invoke(new object[] { 12 });
            Clavier c3 = (Clavier)Activator.CreateInstance(t);
            Clavier c4 = (Clavier)Activator.CreateInstance(t, 30);
            if (c2.Leds != 12 || c3.Leds != 104 || c4.Leds != 30 || Activator.CreateInstance<List<int>>().Count != 0) return 6;
            if ((int)Activator.CreateInstance(typeof(int)) != 0) return 6;

            object[] marques = t.GetCustomAttributes(typeof(MarqueAttribute), false);
            if (marques.Length != 1) return 7;
            MarqueAttribute m = (MarqueAttribute)marques[0];
            if (m.Texte != "clavier" || m.Niveau != 3 || m.Cible != typeof(int)) return 7;
            if (!ajouter.IsDefined(typeof(MarqueAttribute), false) || Attribute.GetCustomAttribute(ajouter, typeof(MarqueAttribute)) == null) return 7;
            AssemblyProductAttribute produit = (AssemblyProductAttribute)Attribute.GetCustomAttribute(t.Assembly, typeof(AssemblyProductAttribute));
            if (produit == null || produit.Product != "Témoin Winlux") return 8;

            Array valeurs = Enum.GetValues(typeof(Couleur));
            if (valeurs.Length != 4 || (Couleur)valeurs.GetValue(3) != Couleur.Bleu) return 9;
            Couleur[] typees = (Couleur[])valeurs;
            if (typees[1] != Couleur.Rouge || Enum.GetNames(typeof(Couleur))[2] != "Vert") return 9;
            if ((Couleur)Enum.Parse(typeof(Couleur), "Rouge, Bleu") != (Couleur.Rouge | Couleur.Bleu)) return 10;
            if ((Couleur)Enum.ToObject(typeof(Couleur), 2) != Couleur.Vert || !Enum.IsDefined(typeof(Couleur), "Bleu") || Enum.IsDefined(typeof(Couleur), 8)) return 10;
            Couleur lu;
            if (!Enum.TryParse("vert", true, out lu) || lu != Couleur.Vert || !(Couleur.Rouge | Couleur.Vert).HasFlag(Couleur.Vert)) return 10;
            if (Enum.GetUnderlyingType(typeof(Couleur)) != typeof(byte) || Enum.GetName(typeof(Couleur), Couleur.Bleu) != "Bleu") return 10;

            if (!typeof(IAppareil).IsAssignableFrom(t) || !t.IsAssignableFrom(typeof(ClavierSansFil)) || typeof(ClavierSansFil).IsAssignableFrom(t)) return 11;
            if (t.GetInterfaces().Length != 1 || t.GetInterface("IAppareil") == null || !typeof(ClavierSansFil).IsSubclassOf(t)) return 11;

            Type liste = typeof(List<string>);
            if (!liste.IsGenericType || liste.GetGenericArguments()[0] != typeof(string)) return 12;
            Type def = liste.GetGenericTypeDefinition();
            if (!def.IsGenericTypeDefinition || def.MakeGenericType(typeof(string)) != liste) return 12;
            object dico = Activator.CreateInstance(typeof(Dictionary<,>).MakeGenericType(typeof(string), typeof(int)));
            if (!(dico is Dictionary<string, int>)) return 12;

            // Poignée forgée par réflexion : ArgumentException, pas de panique.
            MethodInfo forgee = t.GetMethod("Ajouter");
            FieldInfo poignee = forgee.GetType().GetField("_h", BindingFlags.NonPublic | BindingFlags.Instance);
            if (poignee == null) return 13;
            poignee.SetValue(forgee, (long)0x7FFF0000FFFFL);
            try { string n = forgee.Name; return 13; }
            catch (ArgumentException) { }

            // Arguments du mauvais type : refusés avant l'appel (#2037).
            object[][] faux = {
                new object[] { "Saluer", null, new int[] { 1, 2 } },
                new object[] { "Saluer", null, 42 },
                new object[] { "Ajouter", c, "x", 1 },
                new object[] { "Ajouter", c, new int[1], 1 },
                new object[] { "Taille", null, new int[3] },
                new object[] { "Taille", null, "abc" },
                new object[] { "Allumees", null, "pas un appareil" },
            };
            foreach (object[] f in faux)
            {
                object[] args = new object[f.Length - 2];
                Array.Copy(f, 2, args, 0, args.Length);
                try { t.GetMethod((string)f[0]).Invoke(f[1], args); return 14; }
                catch (TargetInvocationException) { return 14; }
                catch (ArgumentException e) { if (e.GetType() != typeof(ArgumentException)) return 14; }
            }
            try { t.GetConstructor(new Type[] { typeof(int) }).Invoke(new object[] { "x" }); return 14; }
            catch (ArgumentException) { }
            // Cible d'un autre type : TargetException ; champ : ArgumentException.
            try { t.GetMethod("Ajouter").Invoke("pas un clavier", new object[] { 1, 2 }); return 14; }
            catch (TargetException) { }
            try { nom.GetValue(new int[2]); return 14; } catch (ArgumentException) { }
            try { nom.SetValue("pas un clavier", "x"); return 14; } catch (ArgumentException) { }
            try { nom.SetValue(c, new int[2]); return 14; } catch (ArgumentException) { }
            nom.SetValue(c, "K95");
            if (c.Nom != "K95" || (string)nom.GetValue(new ClavierSansFil()) != "K70") return 14;
            // Les bons types passent toujours, null et l'élargissement compris.
            if ((int)t.GetMethod("Taille").Invoke(null, new object[] { new byte[3] }) != 3) return 14;
            if ((int)t.GetMethod("Allumees").Invoke(null, new object[] { new ClavierSansFil() }) != 104) return 14;
            if ((long)t.GetMethod("Doubler").Invoke(null, new object[] { 21 }) != 42) return 14;
            if ((string)t.GetMethod("Saluer").Invoke(null, new object[] { null }) != "bonjour ") return 14;
            return 200;
        }
    }
}
