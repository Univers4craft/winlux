// Témoin de System.Version lue en texte (#2842).
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 7) de la première vérification qui échoue.
// Un téléchargeur compare la version de son manifeste (texte) à celle d'un
// fichier installé : Version(string), Parse, TryParse, ToString(n).
using System;

static class VersionTexte
{
    static bool Leve<T>(Action a) where T : Exception
    {
        try { a(); return false; }
        catch (Exception e) { return e.GetType() == typeof(T); }
    }

    static int Main()
    {
        // 1. Deux à quatre composants, espaces admis autour des nombres.
        Version v = new Version("5.1.12.0");
        if (v.Major != 5 || v.Minor != 1 || v.Build != 12 || v.Revision != 0) return 1;
        Version d = Version.Parse(" 4 . 7 ");
        if (d.Major != 4 || d.Minor != 7 || d.Build != -1 || d.Revision != -1) return 1;
        if (Version.Parse("1.2.3").ToString() != "1.2.3") return 1;

        // 2. Comparaison avec une version lue en texte.
        if (!(Version.Parse("5.1.12") > Version.Parse("5.1.9")) || Version.Parse("5.1") != new Version(5, 1)) return 2;

        // 3. Les exceptions de Learn « Version.Parse ».
        if (!Leve<ArgumentNullException>(delegate { Version.Parse(null); })) return 3;
        if (!Leve<ArgumentException>(delegate { Version.Parse("5"); })) return 3;
        if (!Leve<ArgumentException>(delegate { Version.Parse("1.2.3.4.5"); })) return 3;
        if (!Leve<ArgumentOutOfRangeException>(delegate { Version.Parse("1.-2"); })) return 3;
        if (!Leve<FormatException>(delegate { Version.Parse("1.x"); })) return 3;
        if (!Leve<FormatException>(delegate { Version.Parse("1."); })) return 3;
        if (!Leve<OverflowException>(delegate { Version.Parse("1.2147483648"); })) return 3;

        // 4. TryParse : false au lieu d'une exception.
        Version t;
        if (!Version.TryParse("2.0.1", out t) || t.Build != 1) return 4;
        if (Version.TryParse("2.x", out t) || t != null) return 4;
        if (Version.TryParse(null, out t) || Version.TryParse("7", out t)) return 4;

        // 5. ToString(n) : les n premiers composants.
        if (v.ToString(2) != "5.1" || v.ToString(4) != "5.1.12.0" || v.ToString(0) != "") return 5;
        if (!Leve<ArgumentException>(delegate { d.ToString(3); })) return 5;

        // 6. Clone, CompareTo(object), IComparable.
        Version c = (Version)v.Clone();
        if ((object)c == (object)v || c != v) return 6;
        if (((IComparable)v).CompareTo(null) != 1 || v.CompareTo((object)new Version(6, 0)) >= 0) return 6;
        if (!Leve<ArgumentException>(delegate { v.CompareTo((object)"5.1"); })) return 6;

        // 7. Les versions se trient.
        Version[] liste = { Version.Parse("2.0"), Version.Parse("1.10"), Version.Parse("1.9.9") };
        Array.Sort(liste);
        if (liste[0].ToString() != "1.9.9" || liste[2].ToString() != "2.0") return 7;
        return 200;
    }
}
