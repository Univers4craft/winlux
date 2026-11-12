// IObserver<T>, IObservable<T>, IProgress<T>, Progress<T> et
// WeakReference de la mscorlib Winlux (#2007).
// Source : la documentation publique de .NET Framework 4.x (MS Learn).
// Notre ramasse-miettes n'a pas encore de références faibles : une
// WeakReference garde sa cible vivante. C'est permis par le contrat de
// .NET (le moment de la collecte n'est jamais garanti) ; seule la mémoire
// de la cible est retenue plus longtemps.
using System.Threading;

namespace System
{
    public interface IObserver<in T>
    {
        void OnNext(T value);
        void OnError(Exception error);
        void OnCompleted();
    }

    public interface IObservable<out T>
    {
        IDisposable Subscribe(IObserver<T> observer);
    }

    public interface IProgress<in T>
    {
        void Report(T value);
    }

    public class Progress<T> : IProgress<T>
    {
        private readonly Action<T> _handler;

        public Progress() { }

        public Progress(Action<T> handler)
        {
            if (handler == null) throw new ArgumentNullException("handler");
            _handler = handler;
        }

        public event EventHandler<T> ProgressChanged;

        // Sans contexte de synchronisation, le rapport est livré sur le fil
        // de l'appelant, comme le fait .NET sans SynchronizationContext
        // installé (le pool de fils).
        protected virtual void OnReport(T value)
        {
            Action<T> handler = _handler;
            EventHandler<T> changed = ProgressChanged;
            if (handler != null) handler(value);
            if (changed != null) changed(this, value);
        }

        void IProgress<T>.Report(T value) { OnReport(value); }
    }

    public class WeakReference
    {
        private object _cible;
        private readonly bool _suivreResurrection;

        public WeakReference(object target) : this(target, false) { }

        public WeakReference(object target, bool trackResurrection)
        {
            _cible = target;
            _suivreResurrection = trackResurrection;
        }

        public virtual bool IsAlive { get { return _cible != null; } }
        public virtual bool TrackResurrection { get { return _suivreResurrection; } }

        public virtual object Target
        {
            get { return _cible; }
            set { _cible = value; }
        }
    }

    public sealed class WeakReference<T> where T : class
    {
        private T _cible;
        private readonly bool _suivreResurrection;

        public WeakReference(T target) : this(target, false) { }

        public WeakReference(T target, bool trackResurrection)
        {
            _cible = target;
            _suivreResurrection = trackResurrection;
        }

        public void SetTarget(T target) { _cible = target; }

        public bool TryGetTarget(out T target)
        {
            target = _cible;
            return target != null;
        }
    }
}
