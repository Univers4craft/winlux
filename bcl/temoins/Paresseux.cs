// Témoin de System.Lazy<T> dans la mscorlib Winlux (#1701).
// Code de sortie attendu : 63.
// Corsair.Service.exe (iCUE 4.18) lisait Lazy<T>.Value.
using System;
using System.Threading;

class Boite
{
    public int N;
    public Boite(int n) { N = n; }
}

static class Paresseux
{
    static int appels;

    static Boite Fabrique()
    {
        appels++;
        return new Boite(42);
    }

    static int Echoue()
    {
        appels++;
        throw new InvalidOperationException("non");
    }

    static int Main()
    {
        int code = 0;
        Lazy<Boite> a = new Lazy<Boite>(Fabrique);
        if (!a.IsValueCreated && appels == 0) code += 1;
        if (a.Value.N == 42 && a.IsValueCreated) code += 2;
        if (object.ReferenceEquals(a.Value, a.Value) && appels == 1) code += 4;
        Lazy<int> b = new Lazy<int>(() => 7 * 6, LazyThreadSafetyMode.None);
        if (b.Value == 42) code += 8;
        appels = 0;
        Lazy<int> c = new Lazy<int>(Echoue);
        int refus = 0;
        for (int i = 0; i < 2; i++)
        {
            try { int v = c.Value; }
            catch (InvalidOperationException) { refus++; }
        }
        // L'exception de la fabrique est mémorisée : relancée sans rappel.
        if (refus == 2 && appels == 1 && !c.IsValueCreated) code += 16;
        Lazy<int> d = new Lazy<int>(() => 5, LazyThreadSafetyMode.PublicationOnly);
        if (d.Value + d.Value == 10) code += 32;
        return code;
    }
}
