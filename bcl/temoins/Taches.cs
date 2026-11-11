// Témoin de Task, async/await et des primitives de synchronisation dans la
// mscorlib Winlux (#1965).
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 16) de la première vérification qui échoue.
// Option de compilation : -optimize+.
// En version optimisée, comme le service d'iCUE, les machines à états des
// méthodes async sont des structures : c'est le cas que ce témoin vérifie.
// Corsair.Service.exe (iCUE 4.18) appelait AsyncTaskMethodBuilder`1 (53
// fois), TaskAwaiter`1, Task::Delay, SemaphoreSlim, WaitHandle::WaitOne et
// RuntimeHelpers::InitializeArray.
using System;
using System.Threading;
using System.Threading.Tasks;

namespace Temoins
{
    static class Taches
    {
        static int s_compteur;

        static async Task<int> Double(int x)
        {
            await Task.Delay(10);
            return x * 2;
        }

        static async Task<int> Chaine()
        {
            int a = await Double(3);
            int b = await Double(a).ConfigureAwait(false);
            await Task.Yield();
            return a + b;
        }

        static async Task Echoue()
        {
            await Task.Delay(1);
            throw new InvalidOperationException("raté");
        }

        static async Task<string> Attrape()
        {
            try
            {
                await Echoue();
                return "rien";
            }
            catch (InvalidOperationException e)
            {
                return e.Message;
            }
        }

        static async Task<int> Synchrone(int x)
        {
            // Aucune attente réelle : la tâche est terminée au retour.
            await Task.FromResult(0);
            return x + 1;
        }

        static async Task Annulable(CancellationToken token)
        {
            await Task.Delay(Timeout.Infinite, token);
        }

        static async Task<int> SousVerrou(SemaphoreSlim verrou, int i)
        {
            await verrou.WaitAsync();
            try
            {
                int avant = s_compteur;
                await Task.Delay(1);
                s_compteur = avant + i;
                return i;
            }
            finally
            {
                verrou.Release();
            }
        }

        static int Main()
        {
            if (Double(21).Result != 42) return 1;
            if (Chaine().Result != 18) return 2;
            if (Attrape().Result != "raté") return 3;
            Task<int> s = Synchrone(4);
            if (!s.IsCompleted || s.Result != 5 || s.Status != TaskStatus.RanToCompletion) return 4;

            Task<int> r = Task.Run(() => 6 * 7);
            Task<int[]> tous = Task.WhenAll(Double(1), Double(2), r);
            int[] res = tous.Result;
            if (res.Length != 3 || res[0] != 2 || res[1] != 4 || res[2] != 42) return 5;

            Task lente = Task.Delay(2000);
            Task<Task> premiere = Task.WhenAny(lente, Task.Delay(5));
            if (premiere.Result == lente || lente.IsCompleted) return 6;

            TaskCompletionSource<string> tcs = new TaskCompletionSource<string>();
            Task<int> longueur = tcs.Task.ContinueWith(t => t.Result.Length);
            ThreadPool.QueueUserWorkItem(_ => tcs.SetResult("bonjour"));
            if (longueur.Result != 7 || tcs.TrySetResult("encore")) return 7;

            try
            {
                Echoue().Wait();
                return 8;
            }
            catch (AggregateException a)
            {
                if (a.InnerExceptions.Count != 1 || !(a.InnerExceptions[0] is InvalidOperationException)) return 8;
            }

            CancellationTokenSource cts = new CancellationTokenSource();
            Task annulee = Annulable(cts.Token);
            cts.CancelAfter(20);
            try
            {
                annulee.Wait();
                return 9;
            }
            catch (AggregateException a)
            {
                if (!annulee.IsCanceled || !(a.InnerExceptions[0] is TaskCanceledException)) return 9;
            }
            if (!cts.IsCancellationRequested || !cts.Token.IsCancellationRequested) return 9;

            SemaphoreSlim verrou = new SemaphoreSlim(1, 1);
            Task<int[]> sous = Task.WhenAll(SousVerrou(verrou, 1), SousVerrou(verrou, 10), SousVerrou(verrou, 100));
            if (sous.Result[2] != 100 || s_compteur != 111 || verrou.CurrentCount != 1) return 10;
            if (!verrou.Wait(0) || verrou.Wait(10)) return 10;
            verrou.Release();

            ManualResetEvent signal = new ManualResetEvent(false);
            AutoResetEvent auto = new AutoResetEvent(false);
            Task.Factory.StartNew(() => { Task.Delay(10).Wait(); signal.Set(); auto.Set(); });
            if (!signal.WaitOne(5000) || !signal.WaitOne(0)) return 11;
            if (!auto.WaitOne(5000) || auto.WaitOne(0)) return 11;
            if (WaitHandle.WaitAny(new WaitHandle[] { auto, signal }) != 1) return 11;

            int ticks = 0;
            ManualResetEvent fini = new ManualResetEvent(false);
            Timer minuterie = null;
            minuterie = new Timer(_ =>
            {
                if (Interlocked.Increment(ref ticks) == 3) fini.Set();
            }, null, 5, 5);
            bool ok = fini.WaitOne(5000);
            minuterie.Dispose();
            if (!ok || ticks < 3) return 12;

            int n = 0;
            Task[] lot = new Task[8];
            for (int i = 0; i < lot.Length; i++) lot[i] = Task.Run(() => { Interlocked.Add(ref n, 5); });
            Task.WaitAll(lot);
            if (n != 40 || Interlocked.CompareExchange(ref n, 1, 40) != 40 || n != 1) return 13;

            Task<int> deballe = Task.Run(async () => { await Task.Delay(1); return 9; });
            if (deballe.Result != 9 || Task.CompletedTask.Status != TaskStatus.RanToCompletion) return 14;
            if (Task.FromException<int>(new TimeoutException()).Exception.InnerException.GetType() != typeof(TimeoutException)) return 14;

            // Initialiseur assez long pour que le compilateur passe par
            // RuntimeHelpers.InitializeArray.
            byte[] table = { 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20 };
            int[] entiers = { 100, 200, 300, 400, 500, 600, 700, 800, -1 };
            long[] longs = { 1L << 40, 2, 3, 4, 5, 6, 7, 8 };
            if (table[0] != 1 || table[19] != 20 || entiers[3] != 400 || entiers[8] != -1 || longs[0] != 1L << 40 || longs[7] != 8) return 15;

            bool appel = false;
            CancellationTokenSource lie = CancellationTokenSource.CreateLinkedTokenSource(cts.Token, CancellationToken.None);
            CancellationTokenSource seule = new CancellationTokenSource();
            seule.Token.Register(() => appel = true);
            seule.Cancel();
            if (!lie.IsCancellationRequested || !appel || CancellationToken.None.CanBeCanceled) return 16;
            return 200;
        }
    }
}
