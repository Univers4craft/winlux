// Témoin de ThreadPool.SetMinThreads et GetMinThreads (#3289).
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 5) de la première vérification qui échoue.
// Avant #3289, SetMinThreads rendait true sans rien retenir et GetMinThreads
// rendait toujours 4.
using System;
using System.Threading;

static class PoolMinimum
{
    static int Main()
    {
        int f, p;
        // 1. Par défaut, le nombre de processeurs (MS Learn).
        ThreadPool.GetMinThreads(out f, out p);
        if (f != Environment.ProcessorCount || p != Environment.ProcessorCount) return 1;
        // 2. Une valeur acceptée est retenue.
        if (!ThreadPool.SetMinThreads(17, 9)) return 2;
        ThreadPool.GetMinThreads(out f, out p);
        if (f != 17 || p != 9) return 3;
        // 4. Négative : false, rien ne change.
        if (ThreadPool.SetMinThreads(-1, 9)) return 4;
        // 5. Au-dessus du maximum : false, rien ne change.
        int maxF, maxP;
        ThreadPool.GetMaxThreads(out maxF, out maxP);
        if (ThreadPool.SetMinThreads(maxF + 1, 1)) return 5;
        ThreadPool.GetMinThreads(out f, out p);
        if (f != 17 || p != 9) return 5;
        return 200;
    }
}
