// Témoin du constructeur statique qui lève (#3249, ECMA-335 §II.10.5.3.3).
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 7) de la première vérification qui échoue.
// Le premier accès doit lever TypeInitializationException, avec l'exception
// d'origine en InnerException, et chaque accès suivant la même instance ;
// un autre fil attend la fin d'un constructeur statique qui dort.
using System;
using System.Threading.Tasks;

class Fautif
{
    public static int Valeur = Lever();
    static int Lever() { throw new InvalidOperationException("cctor"); }
}

class Lent
{
    public static string Pret;
    static Lent()
    {
        Task.Delay(200).Wait();
        Pret = "oui";
    }
}

static class InitialisationEchouee
{
    static TypeInitializationException Acceder()
    {
        try { int v = Fautif.Valeur; return null; }
        catch (TypeInitializationException e) { return e; }
    }

    static int Main()
    {
        TypeInitializationException a = Acceder();
        if (a == null) return 1;
        if (!(a.InnerException is InvalidOperationException)) return 2;
        if (a.TypeName != "Fautif") return 3;
        TypeInitializationException b = Acceder();
        if (b == null) return 4;
        if (!object.ReferenceEquals(a, b)) return 5;

        // Un second fil touche Lent pendant que le premier exécute son
        // constructeur statique (qui rend la main en attendant).
        Task<string> t = Task.Run(() => { Task.Delay(50).Wait(); return Lent.Pret; });
        string ici = Lent.Pret;
        string vu = t.Result;
        if (ici != "oui") return 6;
        if (vu != "oui") return 7;
        return 200;
    }
}
