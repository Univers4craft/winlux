// Témoin de Uri.TryCreate et des adresses relatives (#2844).
// Façade de compilation : System.
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 6) de la première vérification qui échoue.
// Un manifeste de téléchargement donne des liens relatifs à son adresse ;
// le téléchargeur les résout par new Uri(base, lien) ou Uri.TryCreate.
extern alias Facade;
using System;
using Uri = Facade::System.Uri;
using UriKind = Facade::System.UriKind;
using UriFormatException = Facade::System.UriFormatException;

static class UriRelative
{
    static int Main()
    {
        // 1. TryCreate : vrai pour une adresse absolue, faux sinon, sans exception.
        Uri u;
        if (!Uri.TryCreate("https://cdn.example/a/b/manifest.json?v=2", UriKind.Absolute, out u) || u.Host != "cdn.example") return 1;
        if (Uri.TryCreate("pas une adresse", UriKind.Absolute, out u) || u != null) return 1;
        if (Uri.TryCreate("https://x/", UriKind.Relative, out u)) return 1;
        if (Uri.TryCreate((string)null, UriKind.RelativeOrAbsolute, out u)) return 1;

        // 2. Une référence relative : IsAbsoluteUri faux, texte d'origine
        //    gardé, propriétés absolues refusées.
        Uri r = new Uri("images/a b.png", UriKind.Relative);
        if (r.IsAbsoluteUri || r.OriginalString != "images/a b.png" || r.ToString() != "images/a b.png") return 2;
        try { string h = r.Host; return 2; } catch (InvalidOperationException) { }
        if (!Uri.TryCreate("../x", UriKind.RelativeOrAbsolute, out u) || u.IsAbsoluteUri) return 2;
        try { new Uri("images/a.png"); return 2; } catch (UriFormatException) { }

        // 3. Résolution RFC 3986 §5.2 (exemples de la §5.4.1, base http://a/b/c/d;p?q).
        Uri b = new Uri("http://a/b/c/d;p?q");
        string[][] cas = {
            new[] { "g", "http://a/b/c/g" }, new[] { "./g", "http://a/b/c/g" }, new[] { "g/", "http://a/b/c/g/" },
            new[] { "/g", "http://a/g" }, new[] { "?y", "http://a/b/c/d;p?y" }, new[] { "g?y", "http://a/b/c/g?y" },
            new[] { "#s", "http://a/b/c/d;p?q#s" }, new[] { "", "http://a/b/c/d;p?q" }, new[] { ".", "http://a/b/c/" },
            new[] { "..", "http://a/b/" }, new[] { "../g", "http://a/b/g" }, new[] { "../..", "http://a/" },
            new[] { "../../g", "http://a/g" }, new[] { "//g", "http://g/" }, new[] { "https://autre/x", "https://autre/x" },
        };
        foreach (string[] c in cas)
            if (new Uri(b, c[0]).AbsoluteUri != c[1]) return 3;

        // 4. Le cas du téléchargeur : un paquet à côté du manifeste.
        Uri m = new Uri("https://cdn.example/pdn/5.1/manifest.json");
        if (new Uri(m, "../paquet.zip").AbsoluteUri != "https://cdn.example/pdn/paquet.zip") return 4;
        if (!Uri.TryCreate(m, "x64/setup.exe", out u) || u.AbsoluteUri != "https://cdn.example/pdn/5.1/x64/setup.exe") return 4;
        if (!Uri.TryCreate(m, new Uri("../y", UriKind.Relative), out u) || u.AbsoluteUri != "https://cdn.example/pdn/y") return 4;

        // 5. Égalité : relative contre relative, jamais contre une absolue.
        if (new Uri("a/b", UriKind.Relative) != new Uri("a/b", UriKind.Relative)) return 5;
        if (new Uri("a/b", UriKind.Relative) == m) return 5;

        // 6. Une base relative est refusée.
        try { new Uri(r, "x"); return 6; } catch (ArgumentOutOfRangeException) { }
        if (Uri.TryCreate(r, "x", out u)) return 6;
        return 200;
    }
}
