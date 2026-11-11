// Témoin : un type du .NET Framework référencé dans « System » (et non
// mscorlib) est servi par notre mscorlib (#1947).
// Façade de compilation : System.
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 3) de la première vérification qui échoue.
// Corsair.Service.exe (iCUE 4.18) appelait Stopwatch.StartNew de System.dll.
extern alias Facade;
using System;
using Stopwatch = Facade::System.Diagnostics.Stopwatch;

static class Facade
{
    static int Main()
    {
        Stopwatch s = Stopwatch.StartNew();
        if ((object)s == null || !s.IsRunning) return 1;
        s.Stop();
        if (s.IsRunning || s.ElapsedTicks < 0) return 2;
        if ((object)Type.GetType("System.Diagnostics.Stopwatch, System") == null) return 3;
        return 200;
    }
}
