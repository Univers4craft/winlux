// Témoin de Mutex et Semaphore nommés et de leurs types de sécurité dans la
// mscorlib Winlux (#1988).
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 11) de la première vérification qui échoue.
// Option de compilation : -optimize+.
// Corsair.Service.exe (iCUE 4.18) appelait Mutex::.ctor (7 fois),
// ReleaseMutex, TryOpenExisting, Get/SetAccessControl, MutexSecurity,
// MutexAccessRule, SecurityIdentifier et Semaphore::.ctor.
using System;
using System.Security.AccessControl;
using System.Security.Principal;
using System.Threading;
using System.Threading.Tasks;

namespace Temoins
{
    static class Synchro
    {
        const string Nom = "Local\\WinluxTemoinMutex1988";

        static int Main()
        {
            bool cree;
            Mutex m = new Mutex(true, Nom, out cree);
            if (!cree) return 1;
            bool cree2;
            Mutex m2 = new Mutex(true, Nom, out cree2);
            if (cree2) return 2;

            Mutex ouvert = Mutex.OpenExisting(Nom);
            Mutex absent;
            if (ouvert == null || Mutex.TryOpenExisting("WinluxTemoinAbsent1988", out absent) || absent != null) return 3;

            // Le fil principal possède le mutex : un autre fil ne l'obtient pas.
            if (Task.Run(() => m2.WaitOne(0)).Result) return 4;
            m.ReleaseMutex();
            bool pris = Task.Run(() =>
            {
                bool ok = ouvert.WaitOne(1000);
                if (ok) ouvert.ReleaseMutex();
                return ok;
            }).Result;
            if (!pris) return 5;

            // Récursion : autant de libérations que d'acquisitions.
            if (!m.WaitOne() || !m2.WaitOne(0)) return 6;
            m.ReleaseMutex();
            if (Task.Run(() => ouvert.WaitOne(0)).Result) return 6;
            m2.ReleaseMutex();
            try
            {
                m.ReleaseMutex();
                return 6;
            }
            catch (ApplicationException) { }

            ManualResetEvent evenement = new ManualResetEvent(false);
            if (WaitHandle.WaitAny(new WaitHandle[] { evenement, m }, 1000) != 1) return 7;
            m.ReleaseMutex();
            if (!WaitHandle.WaitAll(new WaitHandle[] { m, m2 }, 1000)) return 7;
            m.ReleaseMutex();
            m2.ReleaseMutex();

            bool semNeuf;
            Semaphore sem = new Semaphore(2, 2, "WinluxTemoinSemaphore1988", out semNeuf);
            if (!semNeuf || !sem.WaitOne(0) || !sem.WaitOne(0) || sem.WaitOne(10)) return 8;
            if (sem.Release(2) != 0) return 8;
            try
            {
                sem.Release();
                return 8;
            }
            catch (SemaphoreFullException) { }
            Semaphore sem2 = Semaphore.OpenExisting("WinluxTemoinSemaphore1988");
            if (!sem2.WaitOne(0) || sem.Release() != 1) return 8;

            try
            {
                Semaphore.OpenExisting(Nom);
                return 9;
            }
            catch (WaitHandleCannotBeOpenedException) { }
            try
            {
                Mutex.OpenExisting("WinluxTemoinAbsent1988");
                return 9;
            }
            catch (WaitHandleCannotBeOpenedException) { }

            SecurityIdentifier tous = new SecurityIdentifier(WellKnownSidType.WorldSid, null);
            MutexSecurity securite = new MutexSecurity();
            securite.AddAccessRule(new MutexAccessRule(tous, MutexRights.FullControl, AccessControlType.Allow));
            bool secNeuf;
            Mutex protege = new Mutex(false, "Global\\WinluxTemoinProtege1988", out secNeuf, securite);
            if (!secNeuf || protege.GetAccessControl() == null) return 10;
            protege.SetAccessControl(new MutexSecurity());
            SemaphoreSecurity semSec = new SemaphoreSecurity();
            semSec.AddAccessRule(new SemaphoreAccessRule(tous, SemaphoreRights.FullControl, AccessControlType.Allow));
            if (tous.Value != "S-1-1-0" || tous.Translate(typeof(NTAccount)).Value != "Everyone") return 10;
            if (!new SecurityIdentifier("S-1-5-32-544").IsWellKnown(WellKnownSidType.BuiltinAdministratorsSid)) return 10;

            protege.Dispose();
            try
            {
                protege.ReleaseMutex();
                return 11;
            }
            catch (ObjectDisposedException) { }
            return 200;
        }
    }
}
