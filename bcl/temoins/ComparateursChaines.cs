// Témoin : System.StringComparer de notre mscorlib (#2774, suivi #2752).
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 9) de la première vérification qui échoue.
// SetupDownloader.exe (installeur web de Paint.NET 5.1.12) s'arrêtait sur
// StringComparer::get_Ordinal après avoir lu sa configuration JSON.
using System;
using System.Collections;
using System.Collections.Generic;

static class ComparateursChaines
{
    static int Main()
    {
        StringComparer o = StringComparer.Ordinal, oi = StringComparer.OrdinalIgnoreCase;
        if (o == null || oi == null || !object.ReferenceEquals(o, StringComparer.Ordinal)) return 1;
        if (o.Equals("Clé", "clé") || !o.Equals("Clé", "Clé") || !oi.Equals("Clé", "CLÉ")) return 2;
        if (o.Compare("a", "b") >= 0 || o.Compare("B", "a") >= 0 || oi.Compare("B", "a") <= 0) return 3;
        if (oi.GetHashCode("version") != oi.GetHashCode("VERSION")) return 4;
        Dictionary<string, int> d = new Dictionary<string, int>(StringComparer.OrdinalIgnoreCase);
        d["ProductVersion"] = 5;
        int v;
        if (!d.TryGetValue("productversion", out v) || v != 5) return 5;
        Dictionary<string, int> e = new Dictionary<string, int>(StringComparer.Ordinal);
        e["ProductVersion"] = 5;
        if (e.ContainsKey("productversion")) return 6;
        IComparer c = o;
        IEqualityComparer q = oi;
        if (c.Compare("a", "b") >= 0 || c.Compare(null, "a") >= 0 || !q.Equals("x", "X") || q.Equals("x", 1)) return 7;
        if (!StringComparer.Create(System.Globalization.CultureInfo.InvariantCulture, true).Equals("Paint", "PAINT")) return 8;
        if (!StringComparer.InvariantCultureIgnoreCase.Equals("abc", "ABC") || StringComparer.CurrentCulture.Equals("abc", "ABC")) return 9;
        return 200;
    }
}
