// Témoin de System.Diagnostics.Stopwatch dans la mscorlib Winlux (#1922).
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 5) de la première vérification qui échoue.
// Corsair.Service.exe (iCUE 4.18) appelait Stopwatch.StartNew.
using System;
using System.Diagnostics;

static class Chrono
{
    static int Main()
    {
        Stopwatch s = Stopwatch.StartNew();
        if (!s.IsRunning) return 1;
        long t0 = Stopwatch.GetTimestamp();
        int x = 0;
        for (int i = 0; i < 200000; i++)
            x += i & 3;
        if (Stopwatch.GetTimestamp() <= t0 || x == 0) return 2;
        s.Stop();
        long fige = s.ElapsedTicks;
        if (fige <= 0 || s.IsRunning || s.ElapsedTicks != fige) return 3;
        if (s.Elapsed.Ticks != fige || s.ElapsedMilliseconds != fige / 10000) return 4;
        s.Reset();
        if (s.ElapsedTicks != 0 || Stopwatch.Frequency != 10000000) return 5;
        return 200;
    }
}
