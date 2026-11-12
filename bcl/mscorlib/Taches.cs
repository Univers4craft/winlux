// Tâches, méthodes async et primitives de synchronisation de la mscorlib
// Winlux (#1965).
//
// Écrit d'après la documentation publique de Microsoft Learn (« Task-based
// Asynchronous Pattern », pages des types Task, TaskAwaiter,
// AsyncTaskMethodBuilder, CancellationToken, SemaphoreSlim, WaitHandle et
// Timer) et ECMA-335 ; aucun code de Microsoft, de Mono ou de Wine.
//
// Modèle :
// - une tâche se termine une seule fois (TrySet*) ; ses continuations
//   s'exécutent alors, de façon synchrone, sur le fil qui la termine ;
// - les attentes bloquantes passent par Monitor.Wait, que le moteur sert en
//   rendant le verrou global (runtime/src/clr/threads.rs) ;
// - le travail de fond passe par ThreadPool.QueueUserWorkItem, servi par le
//   moteur ;
// - il n'y a pas de SynchronizationContext : une méthode async reprend sur
//   le fil qui termine la tâche attendue.
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Runtime.CompilerServices;
using System.Threading;
using System.Threading.Tasks;

namespace System
{
    public class OperationCanceledException : SystemException
    {
        private CancellationToken _token;

        public OperationCanceledException() : base("The operation was canceled.") { }
        public OperationCanceledException(string message) : base(message) { }
        public OperationCanceledException(string message, Exception innerException) : base(message, innerException) { }
        public OperationCanceledException(CancellationToken token) : base("The operation was canceled.") { _token = token; }
        public OperationCanceledException(string message, CancellationToken token) : base(message) { _token = token; }
        public OperationCanceledException(string message, Exception innerException, CancellationToken token)
            : base(message, innerException) { _token = token; }

        public CancellationToken CancellationToken { get { return _token; } }
    }

    public class TimeoutException : SystemException
    {
        public TimeoutException() : base("The operation has timed out.") { }
        public TimeoutException(string message) : base(message) { }
        public TimeoutException(string message, Exception innerException) : base(message, innerException) { }
    }

    public class AggregateException : Exception
    {
        private readonly ReadOnlyCollection<Exception> _inner;

        public AggregateException() : this("One or more errors occurred.", new Exception[0]) { }
        public AggregateException(string message) : this(message, new Exception[0]) { }
        public AggregateException(params Exception[] innerExceptions) : this("One or more errors occurred.", innerExceptions) { }
        public AggregateException(IEnumerable<Exception> innerExceptions) : this("One or more errors occurred.", innerExceptions) { }
        public AggregateException(string message, Exception innerException) : this(message, new Exception[] { innerException }) { }

        public AggregateException(string message, params Exception[] innerExceptions)
            : base(message, innerExceptions != null && innerExceptions.Length > 0 ? innerExceptions[0] : null)
        {
            if (innerExceptions == null) throw new ArgumentNullException("innerExceptions");
            List<Exception> copie = new List<Exception>(innerExceptions.Length);
            for (int i = 0; i < innerExceptions.Length; i++)
            {
                if (innerExceptions[i] == null) throw new ArgumentException("An element of innerExceptions was null.");
                copie.Add(innerExceptions[i]);
            }
            _inner = new ReadOnlyCollection<Exception>(copie);
        }

        public AggregateException(string message, IEnumerable<Exception> innerExceptions)
            : this(message, ToArray(innerExceptions)) { }

        private static Exception[] ToArray(IEnumerable<Exception> items)
        {
            if (items == null) throw new ArgumentNullException("innerExceptions");
            List<Exception> l = new List<Exception>();
            foreach (Exception e in items) l.Add(e);
            return l.ToArray();
        }

        public ReadOnlyCollection<Exception> InnerExceptions { get { return _inner; } }

        public AggregateException Flatten()
        {
            List<Exception> l = new List<Exception>();
            AddFlat(this, l);
            return new AggregateException(Message, l.ToArray());
        }

        private static void AddFlat(AggregateException a, List<Exception> l)
        {
            foreach (Exception e in a._inner)
            {
                AggregateException sous = e as AggregateException;
                if (sous != null) AddFlat(sous, l); else l.Add(e);
            }
        }

        public Exception GetBaseException()
        {
            Exception e = this;
            while (e.InnerException != null)
            {
                AggregateException a = e as AggregateException;
                if (a != null && a._inner.Count != 1) return e;
                e = e.InnerException;
            }
            return e;
        }

        public void Handle(Func<Exception, bool> predicate)
        {
            if (predicate == null) throw new ArgumentNullException("predicate");
            List<Exception> reste = new List<Exception>();
            foreach (Exception e in _inner) if (!predicate(e)) reste.Add(e);
            if (reste.Count > 0) throw new AggregateException(Message, reste.ToArray());
        }
    }
}

namespace System.Collections.ObjectModel
{
    public class ReadOnlyCollection<T> : IList<T>, IReadOnlyList<T>
    {
        private readonly IList<T> _list;

        public ReadOnlyCollection(IList<T> list)
        {
            if (list == null) throw new ArgumentNullException("list");
            _list = list;
        }

        public int Count { get { return _list.Count; } }
        public T this[int index] { get { return _list[index]; } }
        T IList<T>.this[int index] { get { return _list[index]; } set { throw ReadOnly(); } }
        public bool IsReadOnly { get { return true; } }
        public bool Contains(T value) { return _list.Contains(value); }
        public void CopyTo(T[] array, int index) { _list.CopyTo(array, index); }
        public int IndexOf(T value) { return _list.IndexOf(value); }
        public IEnumerator<T> GetEnumerator() { return _list.GetEnumerator(); }
        System.Collections.IEnumerator System.Collections.IEnumerable.GetEnumerator() { return _list.GetEnumerator(); }
        protected IList<T> Items { get { return _list; } }

        private static NotSupportedException ReadOnly() { return new NotSupportedException("Collection is read-only."); }
        void ICollection<T>.Add(T item) { throw ReadOnly(); }
        void ICollection<T>.Clear() { throw ReadOnly(); }
        bool ICollection<T>.Remove(T item) { throw ReadOnly(); }
        void IList<T>.Insert(int index, T item) { throw ReadOnly(); }
        void IList<T>.RemoveAt(int index) { throw ReadOnly(); }
    }
}

namespace System.Threading
{
    public delegate void WaitCallback(object state);
    public delegate void TimerCallback(object state);

    public static class Timeout
    {
        public const int Infinite = -1;
        public static readonly TimeSpan InfiniteTimeSpan = new TimeSpan(0, 0, 0, 0, -1);
    }

    public static class ThreadPool
    {
        // Servi par le moteur (threads.rs) : un fil de fond par élément.
        [MethodImpl(MethodImplOptions.InternalCall)]
        public static extern bool QueueUserWorkItem(WaitCallback callBack);

        [MethodImpl(MethodImplOptions.InternalCall)]
        public static extern bool QueueUserWorkItem(WaitCallback callBack, object state);

        public static bool UnsafeQueueUserWorkItem(WaitCallback callBack, object state)
        {
            return QueueUserWorkItem(callBack, state);
        }

        public static bool SetMinThreads(int workerThreads, int completionPortThreads) { return true; }

        public static void GetMinThreads(out int workerThreads, out int completionPortThreads)
        {
            workerThreads = 4;
            completionPortThreads = 4;
        }

        public static void GetMaxThreads(out int workerThreads, out int completionPortThreads)
        {
            workerThreads = 32767;
            completionPortThreads = 1000;
        }
    }

    public static class Interlocked
    {
        // Servis par le moteur (delegates.rs).
        [MethodImpl(MethodImplOptions.InternalCall)] public static extern int Increment(ref int location);
        [MethodImpl(MethodImplOptions.InternalCall)] public static extern long Increment(ref long location);
        [MethodImpl(MethodImplOptions.InternalCall)] public static extern int Decrement(ref int location);
        [MethodImpl(MethodImplOptions.InternalCall)] public static extern long Decrement(ref long location);
        [MethodImpl(MethodImplOptions.InternalCall)] public static extern int Add(ref int location, int value);
        [MethodImpl(MethodImplOptions.InternalCall)] public static extern long Add(ref long location, long value);
        [MethodImpl(MethodImplOptions.InternalCall)] public static extern int Exchange(ref int location, int value);
        [MethodImpl(MethodImplOptions.InternalCall)] public static extern long Exchange(ref long location, long value);
        [MethodImpl(MethodImplOptions.InternalCall)] public static extern double Exchange(ref double location, double value);
        [MethodImpl(MethodImplOptions.InternalCall)] public static extern IntPtr Exchange(ref IntPtr location, IntPtr value);
        [MethodImpl(MethodImplOptions.InternalCall)] public static extern object Exchange(ref object location, object value);
        [MethodImpl(MethodImplOptions.InternalCall)] public static extern T Exchange<T>(ref T location, T value) where T : class;
        [MethodImpl(MethodImplOptions.InternalCall)] public static extern int CompareExchange(ref int location, int value, int comparand);
        [MethodImpl(MethodImplOptions.InternalCall)] public static extern long CompareExchange(ref long location, long value, long comparand);
        [MethodImpl(MethodImplOptions.InternalCall)] public static extern double CompareExchange(ref double location, double value, double comparand);
        [MethodImpl(MethodImplOptions.InternalCall)] public static extern IntPtr CompareExchange(ref IntPtr location, IntPtr value, IntPtr comparand);
        [MethodImpl(MethodImplOptions.InternalCall)] public static extern object CompareExchange(ref object location, object value, object comparand);
        [MethodImpl(MethodImplOptions.InternalCall)] public static extern T CompareExchange<T>(ref T location, T value, T comparand) where T : class;
        [MethodImpl(MethodImplOptions.InternalCall)] public static extern long Read(ref long location);

        // Le moteur exécute un seul fil managé à la fois (verrou global) :
        // les accès sont déjà ordonnés.
        public static void MemoryBarrier() { }
    }

    public static class Volatile
    {
        public static bool Read(ref bool location) { return location; }
        public static int Read(ref int location) { return location; }
        public static long Read(ref long location) { return location; }
        public static T Read<T>(ref T location) where T : class { return location; }
        public static void Write(ref bool location, bool value) { location = value; }
        public static void Write(ref int location, int value) { location = value; }
        public static void Write(ref long location, long value) { location = value; }
        public static void Write<T>(ref T location, T value) where T : class { location = value; }
    }

    internal static class Horloge2
    {
        // Millisecondes monotones suffisantes pour les délais d'attente.
        internal static long NowMs() { return DateTime.UtcNow.Ticks / TimeSpan.TicksPerMillisecond; }

        internal static int Remaining(long deadline)
        {
            if (deadline < 0) return Timeout.Infinite;
            long r = deadline - NowMs();
            return r <= 0 ? 0 : (r > int.MaxValue ? int.MaxValue : (int)r);
        }

        internal static long Deadline(int millisecondsTimeout)
        {
            if (millisecondsTimeout < -1) throw new ArgumentOutOfRangeException("millisecondsTimeout");
            return millisecondsTimeout == Timeout.Infinite ? -1 : NowMs() + millisecondsTimeout;
        }

        internal static int ToMs(TimeSpan t)
        {
            long ms = (long)t.TotalMilliseconds;
            if (ms < -1 || ms > int.MaxValue) throw new ArgumentOutOfRangeException("timeout");
            return (int)ms;
        }
    }

    // --- Annulation ---

    public struct CancellationToken
    {
        internal readonly CancellationTokenSource _source;

        internal CancellationToken(CancellationTokenSource source) { _source = source; }

        public CancellationToken(bool canceled)
        {
            _source = canceled ? CancellationTokenSource.Canceled : null;
        }

        public static CancellationToken None { get { return default(CancellationToken); } }
        public bool IsCancellationRequested { get { return _source != null && _source.IsCancellationRequested; } }
        public bool CanBeCanceled { get { return _source != null; } }

        public WaitHandle WaitHandle
        {
            get
            {
                if (_source == null) return new ManualResetEvent(false);
                return _source.WaitHandle;
            }
        }

        public void ThrowIfCancellationRequested()
        {
            if (IsCancellationRequested) throw new OperationCanceledException("The operation was canceled.", this);
        }

        public CancellationTokenRegistration Register(Action callback)
        {
            if (callback == null) throw new ArgumentNullException("callback");
            if (_source == null) return default(CancellationTokenRegistration);
            return _source.Register(callback);
        }

        public CancellationTokenRegistration Register(Action callback, bool useSynchronizationContext)
        {
            return Register(callback);
        }

        public CancellationTokenRegistration Register(Action<object> callback, object state)
        {
            if (callback == null) throw new ArgumentNullException("callback");
            return Register(new EtatRappel(callback, state).Run);
        }

        private sealed class EtatRappel
        {
            private readonly Action<object> _cb;
            private readonly object _state;
            public EtatRappel(Action<object> cb, object state) { _cb = cb; _state = state; }
            public void Run() { _cb(_state); }
        }

        public bool Equals(CancellationToken other) { return _source == other._source; }
        public override bool Equals(object other) { return other is CancellationToken && Equals((CancellationToken)other); }
        public override int GetHashCode() { return _source == null ? 0 : _source.GetHashCode(); }
        public static bool operator ==(CancellationToken left, CancellationToken right) { return left.Equals(right); }
        public static bool operator !=(CancellationToken left, CancellationToken right) { return !left.Equals(right); }
    }

    public struct CancellationTokenRegistration : IDisposable
    {
        private readonly CancellationTokenSource _source;
        private readonly Action _callback;

        internal CancellationTokenRegistration(CancellationTokenSource source, Action callback)
        {
            _source = source;
            _callback = callback;
        }

        public void Dispose()
        {
            if (_source != null) _source.Unregister(_callback);
        }
    }

    public class CancellationTokenSource : IDisposable
    {
        private readonly object _lock = new object();
        private List<Action> _callbacks = new List<Action>();
        private bool _canceled;
        private bool _disposed;
        private ManualResetEvent _handle;
        private Timer _timer;

        internal static readonly CancellationTokenSource Canceled = CreateCanceled();

        private static CancellationTokenSource CreateCanceled()
        {
            CancellationTokenSource s = new CancellationTokenSource();
            s._canceled = true;
            return s;
        }

        public CancellationTokenSource() { }
        public CancellationTokenSource(int millisecondsDelay) { CancelAfter(millisecondsDelay); }
        public CancellationTokenSource(TimeSpan delay) { CancelAfter(delay); }

        public bool IsCancellationRequested { get { return _canceled; } }
        public CancellationToken Token
        {
            get
            {
                if (_disposed) throw new ObjectDisposedException(null, "The CancellationTokenSource has been disposed.");
                return new CancellationToken(this);
            }
        }

        internal WaitHandle WaitHandle
        {
            get
            {
                lock (_lock)
                {
                    if (_handle == null) _handle = new ManualResetEvent(_canceled);
                    return _handle;
                }
            }
        }

        public void Cancel() { Cancel(false); }

        public void Cancel(bool throwOnFirstException)
        {
            List<Action> todo;
            lock (_lock)
            {
                if (_canceled) return;
                _canceled = true;
                todo = _callbacks;
                _callbacks = new List<Action>();
                if (_handle != null) _handle.Set();
            }
            List<Exception> erreurs = null;
            // Les rappels s'exécutent du plus récent au plus ancien.
            for (int i = todo.Count - 1; i >= 0; i--)
            {
                try
                {
                    todo[i]();
                }
                catch (Exception e)
                {
                    if (throwOnFirstException) throw;
                    if (erreurs == null) erreurs = new List<Exception>();
                    erreurs.Add(e);
                }
            }
            if (erreurs != null) throw new AggregateException(erreurs);
        }

        public void CancelAfter(int millisecondsDelay)
        {
            if (millisecondsDelay < -1) throw new ArgumentOutOfRangeException("millisecondsDelay");
            if (_disposed) throw new ObjectDisposedException(null, "The CancellationTokenSource has been disposed.");
            if (_canceled) return;
            lock (_lock)
            {
                if (_timer == null)
                {
                    _timer = new Timer(TimerFired, null, millisecondsDelay, Timeout.Infinite);
                    return;
                }
            }
            _timer.Change(millisecondsDelay, Timeout.Infinite);
        }

        public void CancelAfter(TimeSpan delay) { CancelAfter(Horloge2.ToMs(delay)); }

        private void TimerFired(object state)
        {
            try { Cancel(); } catch (Exception) { }
        }

        internal CancellationTokenRegistration Register(Action callback)
        {
            lock (_lock)
            {
                if (!_canceled)
                {
                    _callbacks.Add(callback);
                    return new CancellationTokenRegistration(this, callback);
                }
            }
            callback();
            return default(CancellationTokenRegistration);
        }

        internal void Unregister(Action callback)
        {
            lock (_lock) _callbacks.Remove(callback);
        }

        public static CancellationTokenSource CreateLinkedTokenSource(CancellationToken token1, CancellationToken token2)
        {
            return CreateLinkedTokenSource(new CancellationToken[] { token1, token2 });
        }

        public static CancellationTokenSource CreateLinkedTokenSource(params CancellationToken[] tokens)
        {
            if (tokens == null) throw new ArgumentNullException("tokens");
            CancellationTokenSource lie = new CancellationTokenSource();
            Action annule = lie.LinkedCancel;
            foreach (CancellationToken t in tokens)
                if (t.CanBeCanceled) t.Register(annule);
            return lie;
        }

        private void LinkedCancel()
        {
            try { Cancel(); } catch (Exception) { }
        }

        public void Dispose()
        {
            Dispose(true);
        }

        protected virtual void Dispose(bool disposing)
        {
            if (_disposed) return;
            _disposed = true;
            if (_timer != null) _timer.Dispose();
        }
    }

    // --- Événements et attentes ---

    public enum EventResetMode
    {
        AutoReset = 0,
        ManualReset = 1,
    }

    public abstract class WaitHandle : IDisposable
    {
        public const int WaitTimeout = 258;

        // Tous les objets attendables partagent un verrou : WaitAny et
        // WaitAll s'y endorment et chaque signal les réveille.
        internal static readonly object s_lock = new object();

        // Objet du noyau (Mutex, Semaphore, #1988) : handle de kernel32,
        // attendu par le noyau ; WaitAny et WaitAll l'interrogent.
        internal long _noyau;

        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern int AttendreNoyau(long handle, int millisecondsTimeout);
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern void FermerNoyau(long handle);

        // Un objet du noyau est pris dès qu'il est interrogé avec succès ;
        // la prise attend ensuite Acquire.
        private bool _pris;

        internal virtual bool IsSignaled()
        {
            if (_noyau == 0) return false;
            if (!_pris) _pris = Resultat(AttendreNoyau(_noyau, 0));
            return _pris;
        }

        internal virtual void Acquire() { _pris = false; }

        // 0 : acquis ; 0x80 : mutex abandonné, acquis quand même ; 258 : délai.
        internal static bool Resultat(int r)
        {
            if (r == 0x80) throw new AbandonedMutexException();
            return r == 0;
        }

        private static int Sommeil(WaitHandle[] waitHandles, int reste)
        {
            for (int i = 0; i < waitHandles.Length; i++)
                if (waitHandles[i]._noyau != 0)
                    return reste == Timeout.Infinite || reste > 10 ? 10 : reste;
            return reste;
        }

        public virtual bool WaitOne() { return WaitOne(Timeout.Infinite); }
        public virtual bool WaitOne(TimeSpan timeout) { return WaitOne(Horloge2.ToMs(timeout)); }
        public virtual bool WaitOne(int millisecondsTimeout, bool exitContext) { return WaitOne(millisecondsTimeout); }
        public virtual bool WaitOne(TimeSpan timeout, bool exitContext) { return WaitOne(Horloge2.ToMs(timeout)); }

        public virtual bool WaitOne(int millisecondsTimeout)
        {
            if (_noyau != 0)
            {
                if (millisecondsTimeout < -1) throw new ArgumentOutOfRangeException("millisecondsTimeout");
                if (_pris) { _pris = false; return true; }
                return Resultat(AttendreNoyau(_noyau, millisecondsTimeout));
            }
            return WaitAny(new WaitHandle[] { this }, millisecondsTimeout) != WaitTimeout;
        }

        public static int WaitAny(WaitHandle[] waitHandles) { return WaitAny(waitHandles, Timeout.Infinite); }
        public static int WaitAny(WaitHandle[] waitHandles, TimeSpan timeout) { return WaitAny(waitHandles, Horloge2.ToMs(timeout)); }
        public static int WaitAny(WaitHandle[] waitHandles, int millisecondsTimeout, bool exitContext) { return WaitAny(waitHandles, millisecondsTimeout); }

        public static int WaitAny(WaitHandle[] waitHandles, int millisecondsTimeout)
        {
            if (waitHandles == null) throw new ArgumentNullException("waitHandles");
            long deadline = Horloge2.Deadline(millisecondsTimeout);
            lock (s_lock)
            {
                while (true)
                {
                    for (int i = 0; i < waitHandles.Length; i++)
                    {
                        if (waitHandles[i].IsSignaled())
                        {
                            waitHandles[i].Acquire();
                            return i;
                        }
                    }
                    int reste = Horloge2.Remaining(deadline);
                    if (reste == 0) return WaitTimeout;
                    Monitor.Wait(s_lock, Sommeil(waitHandles, reste));
                }
            }
        }

        public static bool WaitAll(WaitHandle[] waitHandles) { return WaitAll(waitHandles, Timeout.Infinite); }
        public static bool WaitAll(WaitHandle[] waitHandles, TimeSpan timeout) { return WaitAll(waitHandles, Horloge2.ToMs(timeout)); }
        public static bool WaitAll(WaitHandle[] waitHandles, int millisecondsTimeout, bool exitContext) { return WaitAll(waitHandles, millisecondsTimeout); }

        public static bool WaitAll(WaitHandle[] waitHandles, int millisecondsTimeout)
        {
            if (waitHandles == null) throw new ArgumentNullException("waitHandles");
            long deadline = Horloge2.Deadline(millisecondsTimeout);
            lock (s_lock)
            {
                while (true)
                {
                    bool tous = true;
                    for (int i = 0; i < waitHandles.Length; i++)
                        if (!waitHandles[i].IsSignaled()) { tous = false; break; }
                    if (tous)
                    {
                        for (int i = 0; i < waitHandles.Length; i++) waitHandles[i].Acquire();
                        return true;
                    }
                    int reste = Horloge2.Remaining(deadline);
                    if (reste == 0) return false;
                    Monitor.Wait(s_lock, Sommeil(waitHandles, reste));
                }
            }
        }

        internal static void Signal()
        {
            lock (s_lock) Monitor.PulseAll(s_lock);
        }

        public virtual void Close() { Dispose(true); }
        public void Dispose() { Dispose(true); }

        protected virtual void Dispose(bool explicitDisposing)
        {
            long h = _noyau;
            _noyau = 0;
            if (h != 0) FermerNoyau(h);
        }

        // Handle du noyau vu comme SafeWaitHandle (#2019). Un objet géré
        // (ManualResetEvent…) n'en a pas : la poignée est alors invalide.
        protected static readonly IntPtr InvalidHandle = new IntPtr(-1);

        public Microsoft.Win32.SafeHandles.SafeWaitHandle SafeWaitHandle
        {
            get { return new Microsoft.Win32.SafeHandles.SafeWaitHandle(_noyau == 0 ? InvalidHandle : new IntPtr(_noyau), false); }
            set { _noyau = value == null || value.IsInvalid ? 0 : value.DangerousGetHandle().ToInt64(); }
        }

        [Obsolete("Use the SafeWaitHandle property instead.")]
        public virtual IntPtr Handle
        {
            get { return _noyau == 0 ? InvalidHandle : new IntPtr(_noyau); }
            set { _noyau = value == InvalidHandle ? 0 : value.ToInt64(); }
        }
    }

    public class EventWaitHandle : WaitHandle
    {
        private bool _set;
        private readonly EventResetMode _mode;

        public EventWaitHandle(bool initialState, EventResetMode mode)
        {
            if (mode != EventResetMode.AutoReset && mode != EventResetMode.ManualReset) throw new ArgumentException("mode");
            _set = initialState;
            _mode = mode;
        }

        internal override bool IsSignaled() { return _set; }

        internal override void Acquire()
        {
            if (_mode == EventResetMode.AutoReset) _set = false;
        }

        public bool Set()
        {
            lock (s_lock)
            {
                _set = true;
                Monitor.PulseAll(s_lock);
            }
            return true;
        }

        public bool Reset()
        {
            lock (s_lock) _set = false;
            return true;
        }
    }

    public sealed class ManualResetEvent : EventWaitHandle
    {
        public ManualResetEvent(bool initialState) : base(initialState, EventResetMode.ManualReset) { }
    }

    public sealed class AutoResetEvent : EventWaitHandle
    {
        public AutoResetEvent(bool initialState) : base(initialState, EventResetMode.AutoReset) { }
    }

    public class ManualResetEventSlim : IDisposable
    {
        private readonly object _lock = new object();
        private bool _set;
        private ManualResetEvent _handle;

        public ManualResetEventSlim() : this(false) { }
        public ManualResetEventSlim(bool initialState) { _set = initialState; }
        public ManualResetEventSlim(bool initialState, int spinCount) { _set = initialState; }

        public bool IsSet { get { return _set; } }
        public int SpinCount { get { return 1; } }

        public WaitHandle WaitHandle
        {
            get
            {
                lock (_lock)
                {
                    if (_handle == null) _handle = new ManualResetEvent(_set);
                    return _handle;
                }
            }
        }

        public void Set()
        {
            lock (_lock)
            {
                _set = true;
                Monitor.PulseAll(_lock);
                if (_handle != null) _handle.Set();
            }
        }

        public void Reset()
        {
            lock (_lock)
            {
                _set = false;
                if (_handle != null) _handle.Reset();
            }
        }

        public void Wait() { Wait(Timeout.Infinite, default(CancellationToken)); }
        public bool Wait(int millisecondsTimeout) { return Wait(millisecondsTimeout, default(CancellationToken)); }
        public bool Wait(TimeSpan timeout) { return Wait(Horloge2.ToMs(timeout), default(CancellationToken)); }
        public void Wait(CancellationToken cancellationToken) { Wait(Timeout.Infinite, cancellationToken); }
        public bool Wait(TimeSpan timeout, CancellationToken cancellationToken) { return Wait(Horloge2.ToMs(timeout), cancellationToken); }

        public bool Wait(int millisecondsTimeout, CancellationToken cancellationToken)
        {
            long deadline = Horloge2.Deadline(millisecondsTimeout);
            CancellationTokenRegistration reg = cancellationToken.Register(Wake);
            try
            {
                lock (_lock)
                {
                    while (!_set)
                    {
                        cancellationToken.ThrowIfCancellationRequested();
                        int reste = Horloge2.Remaining(deadline);
                        if (reste == 0) return false;
                        Monitor.Wait(_lock, reste);
                    }
                    return true;
                }
            }
            finally
            {
                reg.Dispose();
            }
        }

        private void Wake()
        {
            lock (_lock) Monitor.PulseAll(_lock);
        }

        public void Dispose() { }
    }

    public class SemaphoreFullException : SystemException
    {
        public SemaphoreFullException() : base("Adding the specified count to the semaphore would cause it to exceed its maximum count.") { }
        public SemaphoreFullException(string message) : base(message) { }
    }

    public class SemaphoreSlim : IDisposable
    {
        private readonly object _lock = new object();
        private int _count;
        private readonly int _max;
        private readonly List<TaskCompletionSource<bool>> _asyncWaiters = new List<TaskCompletionSource<bool>>();

        public SemaphoreSlim(int initialCount) : this(initialCount, int.MaxValue) { }

        public SemaphoreSlim(int initialCount, int maxCount)
        {
            if (initialCount < 0 || initialCount > maxCount) throw new ArgumentOutOfRangeException("initialCount");
            if (maxCount <= 0) throw new ArgumentOutOfRangeException("maxCount");
            _count = initialCount;
            _max = maxCount;
        }

        public int CurrentCount { get { return _count; } }

        public WaitHandle AvailableWaitHandle { get { return new ManualResetEvent(_count > 0); } }

        public void Wait() { Wait(Timeout.Infinite, default(CancellationToken)); }
        public bool Wait(int millisecondsTimeout) { return Wait(millisecondsTimeout, default(CancellationToken)); }
        public bool Wait(TimeSpan timeout) { return Wait(Horloge2.ToMs(timeout), default(CancellationToken)); }
        public void Wait(CancellationToken cancellationToken) { Wait(Timeout.Infinite, cancellationToken); }
        public bool Wait(TimeSpan timeout, CancellationToken cancellationToken) { return Wait(Horloge2.ToMs(timeout), cancellationToken); }

        public bool Wait(int millisecondsTimeout, CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            long deadline = Horloge2.Deadline(millisecondsTimeout);
            CancellationTokenRegistration reg = cancellationToken.Register(Wake);
            try
            {
                lock (_lock)
                {
                    while (_count == 0)
                    {
                        cancellationToken.ThrowIfCancellationRequested();
                        int reste = Horloge2.Remaining(deadline);
                        if (reste == 0) return false;
                        Monitor.Wait(_lock, reste);
                    }
                    _count--;
                    return true;
                }
            }
            finally
            {
                reg.Dispose();
            }
        }

        private void Wake()
        {
            lock (_lock) Monitor.PulseAll(_lock);
        }

        public Task WaitAsync() { return WaitAsync(Timeout.Infinite, default(CancellationToken)); }
        public Task WaitAsync(CancellationToken cancellationToken) { return WaitAsync(Timeout.Infinite, cancellationToken); }
        public Task<bool> WaitAsync(int millisecondsTimeout) { return WaitAsync(millisecondsTimeout, default(CancellationToken)); }
        public Task<bool> WaitAsync(TimeSpan timeout) { return WaitAsync(Horloge2.ToMs(timeout), default(CancellationToken)); }
        public Task<bool> WaitAsync(TimeSpan timeout, CancellationToken cancellationToken) { return WaitAsync(Horloge2.ToMs(timeout), cancellationToken); }

        public Task<bool> WaitAsync(int millisecondsTimeout, CancellationToken cancellationToken)
        {
            if (millisecondsTimeout < -1) throw new ArgumentOutOfRangeException("millisecondsTimeout");
            if (cancellationToken.IsCancellationRequested) return Task.FromCanceled<bool>(cancellationToken);
            TaskCompletionSource<bool> attente;
            lock (_lock)
            {
                if (_count > 0)
                {
                    _count--;
                    return Task.FromResult(true);
                }
                if (millisecondsTimeout == 0) return Task.FromResult(false);
                attente = new TaskCompletionSource<bool>();
                _asyncWaiters.Add(attente);
            }
            if (millisecondsTimeout != Timeout.Infinite || cancellationToken.CanBeCanceled)
                new AttenteLimitee(this, attente, millisecondsTimeout, cancellationToken);
            return attente.Task;
        }

        // Fin d'une attente asynchrone par délai ou annulation : l'attente
        // n'est retirée de la file que si Release ne l'a pas déjà servie.
        private sealed class AttenteLimitee
        {
            private readonly SemaphoreSlim _sem;
            private readonly TaskCompletionSource<bool> _tcs;
            private readonly CancellationToken _token;

            public AttenteLimitee(SemaphoreSlim sem, TaskCompletionSource<bool> tcs, int ms, CancellationToken token)
            {
                _sem = sem;
                _tcs = tcs;
                _token = token;
                if (token.CanBeCanceled) token.Register(Canceled);
                if (ms != Timeout.Infinite) Task.Delay(ms).ContinueWith(TimedOut);
            }

            private bool Remove()
            {
                lock (_sem._lock) return _sem._asyncWaiters.Remove(_tcs);
            }

            private void Canceled()
            {
                if (Remove()) _tcs.TrySetCanceled(_token);
            }

            private void TimedOut(Task t)
            {
                if (Remove()) _tcs.TrySetResult(false);
            }
        }

        public int Release() { return Release(1); }

        public int Release(int releaseCount)
        {
            if (releaseCount < 1) throw new ArgumentOutOfRangeException("releaseCount");
            List<TaskCompletionSource<bool>> servis = null;
            int avant;
            lock (_lock)
            {
                avant = _count;
                if (_max - _count < releaseCount) throw new SemaphoreFullException();
                int n = releaseCount;
                while (n > 0 && _asyncWaiters.Count > 0)
                {
                    if (servis == null) servis = new List<TaskCompletionSource<bool>>();
                    servis.Add(_asyncWaiters[0]);
                    _asyncWaiters.RemoveAt(0);
                    n--;
                }
                _count += n;
                if (n > 0) Monitor.PulseAll(_lock);
            }
            if (servis != null)
                foreach (TaskCompletionSource<bool> t in servis) t.TrySetResult(true);
            return avant;
        }

        public void Dispose() { }
    }

    // --- Minuterie ---

    public sealed class Timer : IDisposable
    {
        private readonly object _lock = new object();
        private readonly TimerCallback _callback;
        private readonly object _state;
        private int _due;
        private int _period;
        private int _generation;
        private bool _disposed;

        public Timer(TimerCallback callback) : this(callback, null, Timeout.Infinite, Timeout.Infinite) { }

        public Timer(TimerCallback callback, object state, int dueTime, int period)
        {
            if (callback == null) throw new ArgumentNullException("callback");
            _callback = callback;
            _state = state;
            Change(dueTime, period);
        }

        public Timer(TimerCallback callback, object state, long dueTime, long period)
            : this(callback, state, CheckMs(dueTime, "dueTime"), CheckMs(period, "period")) { }

        public Timer(TimerCallback callback, object state, TimeSpan dueTime, TimeSpan period)
            : this(callback, state, Horloge2.ToMs(dueTime), Horloge2.ToMs(period)) { }

        public Timer(TimerCallback callback, object state, uint dueTime, uint period)
            : this(callback, state, (int)dueTime, (int)period) { }

        private static int CheckMs(long ms, string name)
        {
            if (ms < -1 || ms > 4294967294L) throw new ArgumentOutOfRangeException(name);
            return ms > int.MaxValue ? int.MaxValue : (int)ms;
        }

        public bool Change(long dueTime, long period) { return Change(CheckMs(dueTime, "dueTime"), CheckMs(period, "period")); }
        public bool Change(TimeSpan dueTime, TimeSpan period) { return Change(Horloge2.ToMs(dueTime), Horloge2.ToMs(period)); }
        public bool Change(uint dueTime, uint period) { return Change((int)dueTime, (int)period); }

        public bool Change(int dueTime, int period)
        {
            if (dueTime < -1) throw new ArgumentOutOfRangeException("dueTime");
            if (period < -1) throw new ArgumentOutOfRangeException("period");
            int generation;
            lock (_lock)
            {
                if (_disposed) throw new ObjectDisposedException(null, "Cannot access a disposed object.");
                _due = dueTime;
                _period = period;
                _generation++;
                generation = _generation;
                Monitor.PulseAll(_lock);
            }
            // Un fil par armement : l'ancien voit la génération changer et
            // s'arrête.
            if (dueTime != Timeout.Infinite) ThreadPool.QueueUserWorkItem(Run, generation);
            return true;
        }

        private void Run(object gen)
        {
            int generation = (int)gen;
            int attente;
            lock (_lock) attente = _due;
            while (true)
            {
                lock (_lock)
                {
                    long deadline = Horloge2.Deadline(attente);
                    while (true)
                    {
                        if (_disposed || _generation != generation) return;
                        int reste = Horloge2.Remaining(deadline);
                        if (reste == 0) break;
                        Monitor.Wait(_lock, reste);
                    }
                    attente = _period;
                }
                try
                {
                    _callback(_state);
                }
                catch (Exception)
                {
                    // Le Framework termine le processus ; une minuterie de
                    // fond qui échoue ne doit pas emporter le moteur.
                }
                if (attente == Timeout.Infinite) return;
            }
        }

        public void Dispose()
        {
            lock (_lock)
            {
                _disposed = true;
                Monitor.PulseAll(_lock);
            }
        }

        public bool Dispose(WaitHandle notifyObject)
        {
            Dispose();
            EventWaitHandle e = notifyObject as EventWaitHandle;
            if (e != null) e.Set();
            return true;
        }
    }
}

namespace System.Threading.Tasks
{
    public enum TaskStatus
    {
        Created = 0,
        WaitingForActivation = 1,
        WaitingToRun = 2,
        Running = 3,
        WaitingForChildrenToComplete = 4,
        RanToCompletion = 5,
        Canceled = 6,
        Faulted = 7,
    }

    [Flags]
    public enum TaskCreationOptions
    {
        None = 0,
        PreferFairness = 1,
        LongRunning = 2,
        AttachedToParent = 4,
        DenyChildAttach = 8,
        HideScheduler = 16,
        RunContinuationsAsynchronously = 64,
    }

    [Flags]
    public enum TaskContinuationOptions
    {
        None = 0,
        PreferFairness = 1,
        LongRunning = 2,
        AttachedToParent = 4,
        DenyChildAttach = 8,
        HideScheduler = 16,
        LazyCancellation = 32,
        RunContinuationsAsynchronously = 64,
        NotOnRanToCompletion = 0x10000,
        NotOnFaulted = 0x20000,
        OnlyOnCanceled = 0x30000,
        NotOnCanceled = 0x40000,
        OnlyOnFaulted = 0x50000,
        OnlyOnRanToCompletion = 0x60000,
        ExecuteSynchronously = 0x80000,
    }

    public class TaskCanceledException : OperationCanceledException
    {
        private readonly Task _task;

        public TaskCanceledException() : base("A task was canceled.") { }
        public TaskCanceledException(string message) : base(message) { }
        public TaskCanceledException(string message, Exception innerException) : base(message, innerException) { }
        public TaskCanceledException(Task task)
            : base("A task was canceled.", task != null ? task.CancellationTokenInternal : default(CancellationToken))
        {
            _task = task;
        }

        public Task Task { get { return _task; } }
    }

    public class TaskSchedulerException : Exception
    {
        public TaskSchedulerException() : base("An exception was thrown by a TaskScheduler.") { }
        public TaskSchedulerException(string message) : base(message) { }
        public TaskSchedulerException(Exception innerException) : base("An exception was thrown by a TaskScheduler.", innerException) { }
    }

    // Seul ordonnanceur : le réservoir de fils du moteur.
    public class TaskScheduler
    {
        private static readonly TaskScheduler s_default = new TaskScheduler();
        private static int s_nextId;
        private readonly int _id = Interlocked.Increment(ref s_nextId);

        protected TaskScheduler() { }

        public static TaskScheduler Default { get { return s_default; } }
        public static TaskScheduler Current { get { return s_default; } }
        public int Id { get { return _id; } }
        public virtual int MaximumConcurrencyLevel { get { return int.MaxValue; } }

        public static TaskScheduler FromCurrentSynchronizationContext()
        {
            return s_default;
        }
    }

    public class Task : IDisposable
    {
        private static int s_nextId;
        private static readonly WaitCallback s_run = RunOnPool;
        private static readonly TaskFactory s_factory = new TaskFactory();
        private static Task s_completed;

        internal readonly object _lock = new object();
        private TaskStatus _status;
        private List<Exception> _exceptions;
        private List<Action> _continuations;
        private int _id;
        private readonly Delegate _action;
        private readonly object _state;
        private CancellationToken _token;
        private readonly TaskCreationOptions _options;

        // Tâche « promesse », terminée par TrySet*.
        internal Task()
        {
            _status = TaskStatus.WaitingForActivation;
        }

        internal Task(TaskStatus status)
        {
            _status = status;
        }

        public Task(Action action) : this(action, null, default(CancellationToken), TaskCreationOptions.None) { }
        public Task(Action action, CancellationToken cancellationToken) : this(action, null, cancellationToken, TaskCreationOptions.None) { }
        public Task(Action action, TaskCreationOptions creationOptions) : this(action, null, default(CancellationToken), creationOptions) { }
        public Task(Action action, CancellationToken cancellationToken, TaskCreationOptions creationOptions) : this(action, null, cancellationToken, creationOptions) { }
        public Task(Action<object> action, object state) : this(action, state, default(CancellationToken), TaskCreationOptions.None) { }
        public Task(Action<object> action, object state, CancellationToken cancellationToken) : this(action, state, cancellationToken, TaskCreationOptions.None) { }
        public Task(Action<object> action, object state, TaskCreationOptions creationOptions) : this(action, state, default(CancellationToken), creationOptions) { }
        public Task(Action<object> action, object state, CancellationToken cancellationToken, TaskCreationOptions creationOptions)
            : this((Delegate)action, state, cancellationToken, creationOptions) { }

        internal Task(Delegate action, object state, CancellationToken cancellationToken, TaskCreationOptions options)
        {
            if (action == null) throw new ArgumentNullException("action");
            _action = action;
            _state = state;
            _token = cancellationToken;
            _options = options;
            _status = TaskStatus.Created;
            if (cancellationToken.IsCancellationRequested) _status = TaskStatus.Canceled;
        }

        public int Id
        {
            get
            {
                if (_id == 0) Interlocked.CompareExchange(ref _id, Interlocked.Increment(ref s_nextId), 0);
                return _id;
            }
        }

        public static int? CurrentId { get { return null; } }
        public TaskStatus Status { get { return _status; } }
        public object AsyncState { get { return _state; } }
        public TaskCreationOptions CreationOptions { get { return _options; } }
        internal CancellationToken CancellationTokenInternal { get { return _token; } }

        public bool IsCompleted
        {
            get
            {
                TaskStatus s = _status;
                return s == TaskStatus.RanToCompletion || s == TaskStatus.Faulted || s == TaskStatus.Canceled;
            }
        }

        public bool IsCanceled { get { return _status == TaskStatus.Canceled; } }
        public bool IsFaulted { get { return _status == TaskStatus.Faulted; } }

        public AggregateException Exception
        {
            get
            {
                if (_status != TaskStatus.Faulted) return null;
                return new AggregateException(_exceptions.ToArray());
            }
        }

        public static TaskFactory Factory { get { return s_factory; } }

        public static Task CompletedTask
        {
            get
            {
                if (s_completed == null) s_completed = new Task(TaskStatus.RanToCompletion);
                return s_completed;
            }
        }

        // --- Exécution ---

        public void Start() { Start(TaskScheduler.Default); }

        public void Start(TaskScheduler scheduler)
        {
            lock (_lock)
            {
                if (_action == null) throw new InvalidOperationException("Start may not be called on a promise-style task.");
                if (_status != TaskStatus.Created) throw new InvalidOperationException("Start may not be called on a task that was already started.");
                _status = TaskStatus.WaitingToRun;
            }
            ThreadPool.QueueUserWorkItem(s_run, this);
        }

        public void RunSynchronously() { RunSynchronously(TaskScheduler.Default); }

        public void RunSynchronously(TaskScheduler scheduler)
        {
            lock (_lock)
            {
                if (_action == null) throw new InvalidOperationException("RunSynchronously may not be called on a promise-style task.");
                if (_status != TaskStatus.Created) throw new InvalidOperationException("RunSynchronously may not be called on a task that was already started.");
                _status = TaskStatus.WaitingToRun;
            }
            Execute();
        }

        private static void RunOnPool(object task)
        {
            ((Task)task).Execute();
        }

        internal void Execute()
        {
            if (_token.IsCancellationRequested)
            {
                TrySetCanceled(_token);
                return;
            }
            _status = TaskStatus.Running;
            try
            {
                InvokeAction();
            }
            catch (OperationCanceledException oce)
            {
                if (_token.IsCancellationRequested && oce.CancellationToken == _token) TrySetCanceled(_token);
                else TrySetException(oce);
            }
            catch (Exception e)
            {
                TrySetException(e);
            }
        }

        // Exécute le délégué et termine la tâche avec son résultat.
        internal virtual void InvokeAction()
        {
            Action a = _action as Action;
            if (a != null) a();
            else ((Action<object>)_action)(_state);
            TrySetResultCore();
        }

        internal Delegate ActionDelegate { get { return _action; } }
        internal object StateObject { get { return _state; } }

        // --- Fin de la tâche ---

        // Passe à l'état final si la tâche ne l'a pas déjà atteint ;
        // « commit » range la valeur sous le verrou.
        internal bool TryComplete(TaskStatus status, Exception error, CancellationToken token)
        {
            List<Action> todo;
            lock (_lock)
            {
                if (IsCompleted) return false;
                if (error != null)
                {
                    _exceptions = new List<Exception>();
                    AggregateException a = error as AggregateException;
                    if (a != null && status == TaskStatus.Faulted)
                        foreach (Exception e in a.InnerExceptions) _exceptions.Add(e);
                    else
                        _exceptions.Add(error);
                }
                if (status == TaskStatus.Canceled) _token = token;
                CommitResult();
                _status = status;
                todo = _continuations;
                _continuations = null;
                Monitor.PulseAll(_lock);
            }
            WaitHandle.Signal();
            if (todo != null)
                foreach (Action c in todo) RunContinuation(c);
            return true;
        }

        internal virtual void CommitResult() { }

        internal bool TrySetResultCore() { return TryComplete(TaskStatus.RanToCompletion, null, default(CancellationToken)); }
        internal bool TrySetException(Exception e) { return TryComplete(TaskStatus.Faulted, e, default(CancellationToken)); }
        internal bool TrySetCanceled(CancellationToken token) { return TryComplete(TaskStatus.Canceled, null, token); }

        private static void RunContinuation(Action c)
        {
            try
            {
                c();
            }
            catch (Exception)
            {
                // Une continuation lève dans sa propre tâche (ContinueWith,
                // méthode async) ; rien ne doit remonter dans celle-ci.
            }
        }

        internal void AddContinuation(Action continuation)
        {
            if (continuation == null) throw new ArgumentNullException("continuation");
            lock (_lock)
            {
                if (!IsCompleted)
                {
                    if (_continuations == null) _continuations = new List<Action>();
                    _continuations.Add(continuation);
                    return;
                }
            }
            continuation();
        }

        // --- Attente ---

        public void Wait() { Wait(Timeout.Infinite, default(CancellationToken)); }
        public bool Wait(int millisecondsTimeout) { return Wait(millisecondsTimeout, default(CancellationToken)); }
        public bool Wait(TimeSpan timeout) { return Wait(Horloge2.ToMs(timeout), default(CancellationToken)); }
        public void Wait(CancellationToken cancellationToken) { Wait(Timeout.Infinite, cancellationToken); }

        public bool Wait(int millisecondsTimeout, CancellationToken cancellationToken)
        {
            if (!WaitCore(millisecondsTimeout, cancellationToken)) return false;
            if (_status == TaskStatus.Faulted) throw Exception;
            if (_status == TaskStatus.Canceled) throw new AggregateException(new TaskCanceledException(this));
            return true;
        }

        internal bool WaitCore(int millisecondsTimeout, CancellationToken cancellationToken)
        {
            if (IsCompleted) return true;
            long deadline = Horloge2.Deadline(millisecondsTimeout);
            CancellationTokenRegistration reg = cancellationToken.Register(PulseWaiters);
            try
            {
                lock (_lock)
                {
                    while (!IsCompleted)
                    {
                        cancellationToken.ThrowIfCancellationRequested();
                        int reste = Horloge2.Remaining(deadline);
                        if (reste == 0) return false;
                        Monitor.Wait(_lock, reste);
                    }
                }
            }
            finally
            {
                reg.Dispose();
            }
            return true;
        }

        private void PulseWaiters()
        {
            lock (_lock) Monitor.PulseAll(_lock);
        }

        // Fin d'un « await » : la première exception, pas l'agrégat.
        internal void ThrowIfNotSuccess()
        {
            if (!IsCompleted) WaitCore(Timeout.Infinite, default(CancellationToken));
            if (_status == TaskStatus.Faulted) throw _exceptions[0];
            if (_status == TaskStatus.Canceled) throw new TaskCanceledException(this);
        }

        public static void WaitAll(params Task[] tasks) { WaitAll(tasks, Timeout.Infinite, default(CancellationToken)); }
        public static bool WaitAll(Task[] tasks, int millisecondsTimeout) { return WaitAll(tasks, millisecondsTimeout, default(CancellationToken)); }
        public static bool WaitAll(Task[] tasks, TimeSpan timeout) { return WaitAll(tasks, Horloge2.ToMs(timeout), default(CancellationToken)); }
        public static void WaitAll(Task[] tasks, CancellationToken cancellationToken) { WaitAll(tasks, Timeout.Infinite, cancellationToken); }

        public static bool WaitAll(Task[] tasks, int millisecondsTimeout, CancellationToken cancellationToken)
        {
            if (tasks == null) throw new ArgumentNullException("tasks");
            long deadline = Horloge2.Deadline(millisecondsTimeout);
            List<Exception> erreurs = null;
            foreach (Task t in tasks)
            {
                if (t == null) throw new ArgumentException("The tasks argument included a null value.");
                if (!t.WaitCore(Horloge2.Remaining(deadline), cancellationToken)) return false;
                if (t._status == TaskStatus.Faulted)
                {
                    if (erreurs == null) erreurs = new List<Exception>();
                    erreurs.AddRange(t._exceptions);
                }
                else if (t._status == TaskStatus.Canceled)
                {
                    if (erreurs == null) erreurs = new List<Exception>();
                    erreurs.Add(new TaskCanceledException(t));
                }
            }
            if (erreurs != null) throw new AggregateException(erreurs);
            return true;
        }

        public static int WaitAny(params Task[] tasks) { return WaitAny(tasks, Timeout.Infinite, default(CancellationToken)); }
        public static int WaitAny(Task[] tasks, int millisecondsTimeout) { return WaitAny(tasks, millisecondsTimeout, default(CancellationToken)); }
        public static int WaitAny(Task[] tasks, TimeSpan timeout) { return WaitAny(tasks, Horloge2.ToMs(timeout), default(CancellationToken)); }
        public static int WaitAny(Task[] tasks, CancellationToken cancellationToken) { return WaitAny(tasks, Timeout.Infinite, cancellationToken); }

        public static int WaitAny(Task[] tasks, int millisecondsTimeout, CancellationToken cancellationToken)
        {
            if (tasks == null) throw new ArgumentNullException("tasks");
            for (int i = 0; i < tasks.Length; i++)
                if (tasks[i].IsCompleted) return i;
            Task<Task> premiere = WhenAny(tasks);
            if (!premiere.WaitCore(millisecondsTimeout, cancellationToken)) return -1;
            return Array.IndexOf(tasks, premiere.Result);
        }

        // --- Fabriques ---

        public static Task Run(Action action) { return Run(action, default(CancellationToken)); }

        public static Task Run(Action action, CancellationToken cancellationToken)
        {
            Task t = new Task(action, cancellationToken);
            if (t._status == TaskStatus.Created) t.Start();
            return t;
        }

        public static Task<TResult> Run<TResult>(Func<TResult> function) { return Run(function, default(CancellationToken)); }

        public static Task<TResult> Run<TResult>(Func<TResult> function, CancellationToken cancellationToken)
        {
            Task<TResult> t = new Task<TResult>(function, cancellationToken);
            if (t.Status == TaskStatus.Created) t.Start();
            return t;
        }

        public static Task Run(Func<Task> function) { return Run(function, default(CancellationToken)); }

        public static Task Run(Func<Task> function, CancellationToken cancellationToken)
        {
            if (function == null) throw new ArgumentNullException("function");
            return Run<Task>(function, cancellationToken).Unwrap();
        }

        public static Task<TResult> Run<TResult>(Func<Task<TResult>> function) { return Run(function, default(CancellationToken)); }

        public static Task<TResult> Run<TResult>(Func<Task<TResult>> function, CancellationToken cancellationToken)
        {
            if (function == null) throw new ArgumentNullException("function");
            return Run<Task<TResult>>(function, cancellationToken).Unwrap();
        }

        public static Task<TResult> FromResult<TResult>(TResult result)
        {
            Task<TResult> t = new Task<TResult>();
            t.TrySetResult(result);
            return t;
        }

        public static Task FromException(Exception exception)
        {
            if (exception == null) throw new ArgumentNullException("exception");
            Task t = new Task();
            t.TrySetException(exception);
            return t;
        }

        public static Task<TResult> FromException<TResult>(Exception exception)
        {
            if (exception == null) throw new ArgumentNullException("exception");
            Task<TResult> t = new Task<TResult>();
            t.TrySetException(exception);
            return t;
        }

        public static Task FromCanceled(CancellationToken cancellationToken)
        {
            Task t = new Task();
            t.TrySetCanceled(cancellationToken);
            return t;
        }

        public static Task<TResult> FromCanceled<TResult>(CancellationToken cancellationToken)
        {
            Task<TResult> t = new Task<TResult>();
            t.TrySetCanceled(cancellationToken);
            return t;
        }

        public static Task Delay(int millisecondsDelay) { return Delay(millisecondsDelay, default(CancellationToken)); }
        public static Task Delay(TimeSpan delay) { return Delay(Horloge2.ToMs(delay), default(CancellationToken)); }
        public static Task Delay(TimeSpan delay, CancellationToken cancellationToken) { return Delay(Horloge2.ToMs(delay), cancellationToken); }

        public static Task Delay(int millisecondsDelay, CancellationToken cancellationToken)
        {
            if (millisecondsDelay < -1) throw new ArgumentOutOfRangeException("millisecondsDelay", "The value needs to be either -1 (signifying an infinite timeout), 0 or a positive integer.");
            if (cancellationToken.IsCancellationRequested) return FromCanceled(cancellationToken);
            if (millisecondsDelay == 0) return CompletedTask;
            return new DelayPromise(millisecondsDelay, cancellationToken);
        }

        private sealed class DelayPromise : Task
        {
            private readonly int _ms;
            private readonly CancellationToken _cancel;

            public DelayPromise(int ms, CancellationToken token)
            {
                _ms = ms;
                _cancel = token;
                if (token.CanBeCanceled) token.Register(OnCancel);
                ThreadPool.QueueUserWorkItem(Sleep);
            }

            private void OnCancel()
            {
                TrySetCanceled(_cancel);
            }

            private void Sleep(object unused)
            {
                // Monitor.Wait sur le verrou de la tâche : une annulation la
                // termine et réveille ce fil.
                long deadline = Horloge2.Deadline(_ms);
                lock (_lock)
                {
                    while (!IsCompleted)
                    {
                        int reste = Horloge2.Remaining(deadline);
                        if (reste == 0) break;
                        Monitor.Wait(_lock, reste);
                    }
                }
                TrySetResultCore();
            }
        }

        public static YieldAwaitable Yield() { return default(YieldAwaitable); }

        // --- Combinateurs ---

        public static Task WhenAll(params Task[] tasks)
        {
            if (tasks == null) throw new ArgumentNullException("tasks");
            return new WhenAllPromise<object>(Copy(tasks), false);
        }

        public static Task WhenAll(IEnumerable<Task> tasks)
        {
            return WhenAll(ToArray(tasks));
        }

        public static Task<TResult[]> WhenAll<TResult>(params Task<TResult>[] tasks)
        {
            if (tasks == null) throw new ArgumentNullException("tasks");
            Task[] copie = new Task[tasks.Length];
            for (int i = 0; i < tasks.Length; i++) copie[i] = tasks[i];
            return new WhenAllPromise<TResult>(Copy(copie), true);
        }

        public static Task<TResult[]> WhenAll<TResult>(IEnumerable<Task<TResult>> tasks)
        {
            if (tasks == null) throw new ArgumentNullException("tasks");
            List<Task<TResult>> l = new List<Task<TResult>>();
            foreach (Task<TResult> t in tasks) l.Add(t);
            return WhenAll(l.ToArray());
        }

        private static Task[] ToArray(IEnumerable<Task> tasks)
        {
            if (tasks == null) throw new ArgumentNullException("tasks");
            List<Task> l = new List<Task>();
            foreach (Task t in tasks) l.Add(t);
            return l.ToArray();
        }

        private static Task[] Copy(Task[] tasks)
        {
            Task[] copie = new Task[tasks.Length];
            for (int i = 0; i < tasks.Length; i++)
            {
                if (tasks[i] == null) throw new ArgumentException("The tasks argument included a null value.", "tasks");
                copie[i] = tasks[i];
            }
            return copie;
        }

        private sealed class WhenAllPromise<TResult> : Task<TResult[]>
        {
            private readonly Task[] _tasks;
            private readonly bool _results;
            private int _remaining;

            public WhenAllPromise(Task[] tasks, bool results)
            {
                _tasks = tasks;
                _results = results;
                _remaining = tasks.Length;
                if (tasks.Length == 0)
                {
                    Finish();
                    return;
                }
                Action un = OneDone;
                foreach (Task t in tasks) t.AddContinuation(un);
            }

            private void OneDone()
            {
                if (Interlocked.Decrement(ref _remaining) == 0) Finish();
            }

            private void Finish()
            {
                List<Exception> erreurs = null;
                Task annulee = null;
                foreach (Task t in _tasks)
                {
                    if (t.IsFaulted)
                    {
                        if (erreurs == null) erreurs = new List<Exception>();
                        erreurs.AddRange(t.Exception.InnerExceptions);
                    }
                    else if (t.IsCanceled && annulee == null)
                    {
                        annulee = t;
                    }
                }
                if (erreurs != null)
                {
                    TrySetException(new AggregateException(erreurs));
                }
                else if (annulee != null)
                {
                    TrySetCanceled(annulee.CancellationTokenInternal);
                }
                else if (!_results)
                {
                    TrySetResult(null);
                }
                else
                {
                    TResult[] r = new TResult[_tasks.Length];
                    for (int i = 0; i < r.Length; i++) r[i] = ((Task<TResult>)_tasks[i]).ResultUnchecked;
                    TrySetResult(r);
                }
            }
        }

        public static Task<Task> WhenAny(params Task[] tasks)
        {
            if (tasks == null) throw new ArgumentNullException("tasks");
            if (tasks.Length == 0) throw new ArgumentException("The tasks argument contains no tasks.", "tasks");
            return new WhenAnyPromise<Task>(Copy(tasks));
        }

        public static Task<Task> WhenAny(IEnumerable<Task> tasks)
        {
            return WhenAny(ToArray(tasks));
        }

        public static Task<Task<TResult>> WhenAny<TResult>(params Task<TResult>[] tasks)
        {
            if (tasks == null) throw new ArgumentNullException("tasks");
            if (tasks.Length == 0) throw new ArgumentException("The tasks argument contains no tasks.", "tasks");
            Task[] copie = new Task[tasks.Length];
            for (int i = 0; i < tasks.Length; i++) copie[i] = tasks[i];
            return new WhenAnyPromise<Task<TResult>>(Copy(copie));
        }

        private sealed class WhenAnyPromise<TTask> : Task<TTask> where TTask : Task
        {
            public WhenAnyPromise(Task[] tasks)
            {
                foreach (Task t in tasks) t.AddContinuation(new Premiere(this, (TTask)t).Run);
            }

            private sealed class Premiere
            {
                private readonly WhenAnyPromise<TTask> _p;
                private readonly TTask _t;
                public Premiere(WhenAnyPromise<TTask> p, TTask t) { _p = p; _t = t; }
                public void Run() { _p.TrySetResult(_t); }
            }
        }

        // --- Continuations ---

        public Task ContinueWith(Action<Task> continuationAction)
        {
            return ContinueWith(continuationAction, default(CancellationToken), TaskContinuationOptions.None, TaskScheduler.Default);
        }

        public Task ContinueWith(Action<Task> continuationAction, CancellationToken cancellationToken)
        {
            return ContinueWith(continuationAction, cancellationToken, TaskContinuationOptions.None, TaskScheduler.Default);
        }

        public Task ContinueWith(Action<Task> continuationAction, TaskContinuationOptions continuationOptions)
        {
            return ContinueWith(continuationAction, default(CancellationToken), continuationOptions, TaskScheduler.Default);
        }

        public Task ContinueWith(Action<Task> continuationAction, TaskScheduler scheduler)
        {
            return ContinueWith(continuationAction, default(CancellationToken), TaskContinuationOptions.None, scheduler);
        }

        public Task ContinueWith(Action<Task> continuationAction, CancellationToken cancellationToken, TaskContinuationOptions continuationOptions, TaskScheduler scheduler)
        {
            if (continuationAction == null) throw new ArgumentNullException("continuationAction");
            Continuation<object> c = new Continuation<object>(this, continuationAction, null, cancellationToken, continuationOptions);
            AddContinuation(c.Run);
            return c;
        }

        public Task<TResult> ContinueWith<TResult>(Func<Task, TResult> continuationFunction)
        {
            return ContinueWith(continuationFunction, default(CancellationToken), TaskContinuationOptions.None, TaskScheduler.Default);
        }

        public Task<TResult> ContinueWith<TResult>(Func<Task, TResult> continuationFunction, CancellationToken cancellationToken)
        {
            return ContinueWith(continuationFunction, cancellationToken, TaskContinuationOptions.None, TaskScheduler.Default);
        }

        public Task<TResult> ContinueWith<TResult>(Func<Task, TResult> continuationFunction, TaskContinuationOptions continuationOptions)
        {
            return ContinueWith(continuationFunction, default(CancellationToken), continuationOptions, TaskScheduler.Default);
        }

        public Task<TResult> ContinueWith<TResult>(Func<Task, TResult> continuationFunction, CancellationToken cancellationToken, TaskContinuationOptions continuationOptions, TaskScheduler scheduler)
        {
            if (continuationFunction == null) throw new ArgumentNullException("continuationFunction");
            Continuation<TResult> c = new Continuation<TResult>(this, null, continuationFunction, cancellationToken, continuationOptions);
            AddContinuation(c.Run);
            return c;
        }

        internal sealed class Continuation<TResult> : Task<TResult>
        {
            private readonly Task _antecedent;
            private readonly Action<Task> _cAction;
            private readonly Func<Task, TResult> _cFunction;
            private readonly CancellationToken _cancel;
            private readonly TaskContinuationOptions _when;

            public Continuation(Task antecedent, Action<Task> action, Func<Task, TResult> function, CancellationToken token, TaskContinuationOptions options)
            {
                _antecedent = antecedent;
                _cAction = action;
                _cFunction = function;
                _cancel = token;
                _when = options;
                if (token.CanBeCanceled) token.Register(OnCancel);
            }

            private void OnCancel()
            {
                TrySetCanceled(_cancel);
            }

            public void Run()
            {
                if (IsCompleted) return;
                TaskContinuationOptions o = _when;
                bool saute =
                    (_antecedent.Status == TaskStatus.RanToCompletion && (o & TaskContinuationOptions.NotOnRanToCompletion) != 0)
                    || (_antecedent.Status == TaskStatus.Faulted && (o & TaskContinuationOptions.NotOnFaulted) != 0)
                    || (_antecedent.Status == TaskStatus.Canceled && (o & TaskContinuationOptions.NotOnCanceled) != 0);
                if (saute)
                {
                    TrySetCanceled(default(CancellationToken));
                    return;
                }
                try
                {
                    if (_cAction != null)
                    {
                        _cAction(_antecedent);
                        TrySetResult(default(TResult));
                    }
                    else
                    {
                        TrySetResult(_cFunction(_antecedent));
                    }
                }
                catch (OperationCanceledException oce)
                {
                    if (_cancel.IsCancellationRequested && oce.CancellationToken == _cancel) TrySetCanceled(_cancel);
                    else TrySetException(oce);
                }
                catch (Exception e)
                {
                    TrySetException(e);
                }
            }
        }

        public TaskAwaiter GetAwaiter() { return new TaskAwaiter(this); }

        public ConfiguredTaskAwaitable ConfigureAwait(bool continueOnCapturedContext)
        {
            return new ConfiguredTaskAwaitable(this);
        }

        public void Dispose() { }
        protected virtual void Dispose(bool disposing) { }
    }

    public class Task<TResult> : Task
    {
        private TResult _result;
        private TResult _pending;

        internal Task() { }

        public Task(Func<TResult> function) : this(function, default(CancellationToken), TaskCreationOptions.None) { }
        public Task(Func<TResult> function, CancellationToken cancellationToken) : this(function, cancellationToken, TaskCreationOptions.None) { }
        public Task(Func<TResult> function, TaskCreationOptions creationOptions) : this(function, default(CancellationToken), creationOptions) { }
        public Task(Func<TResult> function, CancellationToken cancellationToken, TaskCreationOptions creationOptions)
            : base(function, null, cancellationToken, creationOptions) { }
        public Task(Func<object, TResult> function, object state) : this(function, state, default(CancellationToken), TaskCreationOptions.None) { }
        public Task(Func<object, TResult> function, object state, CancellationToken cancellationToken) : this(function, state, cancellationToken, TaskCreationOptions.None) { }
        public Task(Func<object, TResult> function, object state, TaskCreationOptions creationOptions) : this(function, state, default(CancellationToken), creationOptions) { }
        public Task(Func<object, TResult> function, object state, CancellationToken cancellationToken, TaskCreationOptions creationOptions)
            : base(function, state, cancellationToken, creationOptions) { }

        public new static TaskFactory<TResult> Factory { get { return new TaskFactory<TResult>(); } }

        internal override void InvokeAction()
        {
            Func<TResult> f = ActionDelegate as Func<TResult>;
            TResult r = f != null ? f() : ((Func<object, TResult>)ActionDelegate)(StateObject);
            TrySetResult(r);
        }

        public TResult Result
        {
            get
            {
                Wait();
                return _result;
            }
        }

        internal TResult ResultUnchecked { get { return _result; } }

        // La valeur est rangée sous le verrou, avant que l'état ne passe à
        // RanToCompletion : un lecteur ne voit jamais l'état sans la valeur.
        internal override void CommitResult()
        {
            _result = _pending;
            _pending = default(TResult);
        }

        internal bool TrySetResult(TResult result)
        {
            lock (_lock)
            {
                if (IsCompleted) return false;
                _pending = result;
            }
            return TrySetResultCore();
        }

        internal TResult GetResultForAwait()
        {
            ThrowIfNotSuccess();
            return _result;
        }

        public Task ContinueWith(Action<Task<TResult>> continuationAction)
        {
            return ContinueWith(continuationAction, default(CancellationToken), TaskContinuationOptions.None, TaskScheduler.Default);
        }

        public Task ContinueWith(Action<Task<TResult>> continuationAction, CancellationToken cancellationToken)
        {
            return ContinueWith(continuationAction, cancellationToken, TaskContinuationOptions.None, TaskScheduler.Default);
        }

        public Task ContinueWith(Action<Task<TResult>> continuationAction, TaskContinuationOptions continuationOptions)
        {
            return ContinueWith(continuationAction, default(CancellationToken), continuationOptions, TaskScheduler.Default);
        }

        public Task ContinueWith(Action<Task<TResult>> continuationAction, CancellationToken cancellationToken, TaskContinuationOptions continuationOptions, TaskScheduler scheduler)
        {
            if (continuationAction == null) throw new ArgumentNullException("continuationAction");
            return base.ContinueWith(new Adaptateur<object>(continuationAction, null).RunAction, cancellationToken, continuationOptions, scheduler);
        }

        public Task<TNewResult> ContinueWith<TNewResult>(Func<Task<TResult>, TNewResult> continuationFunction)
        {
            return ContinueWith(continuationFunction, default(CancellationToken), TaskContinuationOptions.None, TaskScheduler.Default);
        }

        public Task<TNewResult> ContinueWith<TNewResult>(Func<Task<TResult>, TNewResult> continuationFunction, CancellationToken cancellationToken)
        {
            return ContinueWith(continuationFunction, cancellationToken, TaskContinuationOptions.None, TaskScheduler.Default);
        }

        public Task<TNewResult> ContinueWith<TNewResult>(Func<Task<TResult>, TNewResult> continuationFunction, TaskContinuationOptions continuationOptions)
        {
            return ContinueWith(continuationFunction, default(CancellationToken), continuationOptions, TaskScheduler.Default);
        }

        public Task<TNewResult> ContinueWith<TNewResult>(Func<Task<TResult>, TNewResult> continuationFunction, CancellationToken cancellationToken, TaskContinuationOptions continuationOptions, TaskScheduler scheduler)
        {
            if (continuationFunction == null) throw new ArgumentNullException("continuationFunction");
            return base.ContinueWith<TNewResult>(new Adaptateur<TNewResult>(null, continuationFunction).RunFunction, cancellationToken, continuationOptions, scheduler);
        }

        // Passe la tâche typée à une continuation écrite pour Task<TResult>.
        private sealed class Adaptateur<TNew>
        {
            private readonly Action<Task<TResult>> _a;
            private readonly Func<Task<TResult>, TNew> _f;
            public Adaptateur(Action<Task<TResult>> a, Func<Task<TResult>, TNew> f) { _a = a; _f = f; }
            public void RunAction(Task t) { _a((Task<TResult>)t); }
            public TNew RunFunction(Task t) { return _f((Task<TResult>)t); }
        }

        public new TaskAwaiter<TResult> GetAwaiter() { return new TaskAwaiter<TResult>(this); }

        public new ConfiguredTaskAwaitable<TResult> ConfigureAwait(bool continueOnCapturedContext)
        {
            return new ConfiguredTaskAwaitable<TResult>(this);
        }
    }

    public static class TaskExtensions
    {
        public static Task Unwrap(this Task<Task> task)
        {
            if (task == null) throw new ArgumentNullException("task");
            return new UnwrapPromise<object>(task, false);
        }

        public static Task<TResult> Unwrap<TResult>(this Task<Task<TResult>> task)
        {
            if (task == null) throw new ArgumentNullException("task");
            return new UnwrapPromise<TResult>(task, true);
        }

        private sealed class UnwrapPromise<TResult> : Task<TResult>
        {
            private readonly Task _outer;
            private readonly bool _typed;

            public UnwrapPromise(Task outer, bool typed)
            {
                _outer = outer;
                _typed = typed;
                outer.AddContinuation(OuterDone);
            }

            private void OuterDone()
            {
                if (_outer.IsFaulted) { TrySetException(_outer.Exception); return; }
                if (_outer.IsCanceled) { TrySetCanceled(_outer.CancellationTokenInternal); return; }
                Task inner = _typed ? (Task)((Task<Task<TResult>>)_outer).ResultUnchecked : ((Task<Task>)_outer).ResultUnchecked;
                if (inner == null) { TrySetCanceled(default(CancellationToken)); return; }
                inner.AddContinuation(new Interne(this, inner).Run);
            }

            private sealed class Interne
            {
                private readonly UnwrapPromise<TResult> _p;
                private readonly Task _inner;
                public Interne(UnwrapPromise<TResult> p, Task inner) { _p = p; _inner = inner; }

                public void Run()
                {
                    if (_inner.IsFaulted) _p.TrySetException(_inner.Exception);
                    else if (_inner.IsCanceled) _p.TrySetCanceled(_inner.CancellationTokenInternal);
                    else if (_p._typed) _p.TrySetResult(((Task<TResult>)_inner).ResultUnchecked);
                    else _p.TrySetResult(default(TResult));
                }
            }
        }
    }

    public class TaskCompletionSource<TResult>
    {
        private readonly Task<TResult> _task;

        public TaskCompletionSource() : this(null, TaskCreationOptions.None) { }
        public TaskCompletionSource(object state) : this(state, TaskCreationOptions.None) { }
        public TaskCompletionSource(TaskCreationOptions creationOptions) : this(null, creationOptions) { }
        public TaskCompletionSource(object state, TaskCreationOptions creationOptions) { _task = new Task<TResult>(); }

        public Task<TResult> Task { get { return _task; } }

        public bool TrySetResult(TResult result) { return _task.TrySetResult(result); }
        public bool TrySetException(Exception exception)
        {
            if (exception == null) throw new ArgumentNullException("exception");
            return _task.TrySetException(exception);
        }
        public bool TrySetException(IEnumerable<Exception> exceptions)
        {
            return _task.TrySetException(new AggregateException(exceptions));
        }
        public bool TrySetCanceled() { return _task.TrySetCanceled(default(CancellationToken)); }
        public bool TrySetCanceled(CancellationToken cancellationToken) { return _task.TrySetCanceled(cancellationToken); }

        public void SetResult(TResult result) { if (!TrySetResult(result)) throw AlreadyDone(); }
        public void SetException(Exception exception) { if (!TrySetException(exception)) throw AlreadyDone(); }
        public void SetException(IEnumerable<Exception> exceptions) { if (!TrySetException(exceptions)) throw AlreadyDone(); }
        public void SetCanceled() { if (!TrySetCanceled()) throw AlreadyDone(); }

        private static InvalidOperationException AlreadyDone()
        {
            return new InvalidOperationException("An attempt was made to transition a task to a final state when it had already completed.");
        }
    }

    public class TaskFactory
    {
        private readonly CancellationToken _token;
        private readonly TaskCreationOptions _options;

        public TaskFactory() { }
        public TaskFactory(CancellationToken cancellationToken) { _token = cancellationToken; }
        public TaskFactory(TaskScheduler scheduler) { }
        public TaskFactory(TaskCreationOptions creationOptions, TaskContinuationOptions continuationOptions) { _options = creationOptions; }
        public TaskFactory(CancellationToken cancellationToken, TaskCreationOptions creationOptions, TaskContinuationOptions continuationOptions, TaskScheduler scheduler)
        {
            _token = cancellationToken;
            _options = creationOptions;
        }

        public CancellationToken CancellationToken { get { return _token; } }
        public TaskCreationOptions CreationOptions { get { return _options; } }
        public TaskScheduler Scheduler { get { return null; } }

        public Task StartNew(Action action) { return StartNew(action, _token, _options, TaskScheduler.Default); }
        public Task StartNew(Action action, CancellationToken cancellationToken) { return StartNew(action, cancellationToken, _options, TaskScheduler.Default); }
        public Task StartNew(Action action, TaskCreationOptions creationOptions) { return StartNew(action, _token, creationOptions, TaskScheduler.Default); }

        public Task StartNew(Action action, CancellationToken cancellationToken, TaskCreationOptions creationOptions, TaskScheduler scheduler)
        {
            Task t = new Task(action, cancellationToken, creationOptions);
            if (t.Status == TaskStatus.Created) t.Start(scheduler);
            return t;
        }

        public Task StartNew(Action<object> action, object state) { return StartNew(action, state, _token, _options, TaskScheduler.Default); }
        public Task StartNew(Action<object> action, object state, CancellationToken cancellationToken) { return StartNew(action, state, cancellationToken, _options, TaskScheduler.Default); }
        public Task StartNew(Action<object> action, object state, TaskCreationOptions creationOptions) { return StartNew(action, state, _token, creationOptions, TaskScheduler.Default); }

        public Task StartNew(Action<object> action, object state, CancellationToken cancellationToken, TaskCreationOptions creationOptions, TaskScheduler scheduler)
        {
            Task t = new Task(action, state, cancellationToken, creationOptions);
            if (t.Status == TaskStatus.Created) t.Start(scheduler);
            return t;
        }

        public Task<TResult> StartNew<TResult>(Func<TResult> function) { return StartNew(function, _token, _options, TaskScheduler.Default); }
        public Task<TResult> StartNew<TResult>(Func<TResult> function, CancellationToken cancellationToken) { return StartNew(function, cancellationToken, _options, TaskScheduler.Default); }
        public Task<TResult> StartNew<TResult>(Func<TResult> function, TaskCreationOptions creationOptions) { return StartNew(function, _token, creationOptions, TaskScheduler.Default); }

        public Task<TResult> StartNew<TResult>(Func<TResult> function, CancellationToken cancellationToken, TaskCreationOptions creationOptions, TaskScheduler scheduler)
        {
            Task<TResult> t = new Task<TResult>(function, cancellationToken, creationOptions);
            if (t.Status == TaskStatus.Created) t.Start(scheduler);
            return t;
        }

        public Task<TResult> StartNew<TResult>(Func<object, TResult> function, object state) { return StartNew(function, state, _token, _options, TaskScheduler.Default); }

        public Task<TResult> StartNew<TResult>(Func<object, TResult> function, object state, CancellationToken cancellationToken, TaskCreationOptions creationOptions, TaskScheduler scheduler)
        {
            Task<TResult> t = new Task<TResult>(function, state, cancellationToken, creationOptions);
            if (t.Status == TaskStatus.Created) t.Start(scheduler);
            return t;
        }

        public Task ContinueWhenAll(Task[] tasks, Action<Task[]> continuationAction)
        {
            if (continuationAction == null) throw new ArgumentNullException("continuationAction");
            return Task.WhenAll(tasks).ContinueWith(new Groupe(tasks, continuationAction).Run);
        }

        private sealed class Groupe
        {
            private readonly Task[] _tasks;
            private readonly Action<Task[]> _a;
            public Groupe(Task[] tasks, Action<Task[]> a) { _tasks = tasks; _a = a; }
            public void Run(Task t) { _a(_tasks); }
        }
    }

    public class TaskFactory<TResult>
    {
        public TaskFactory() { }

        public Task<TResult> StartNew(Func<TResult> function) { return Task.Factory.StartNew(function); }

        public Task<TResult> StartNew(Func<TResult> function, CancellationToken cancellationToken, TaskCreationOptions creationOptions, TaskScheduler scheduler)
        {
            return Task.Factory.StartNew(function, cancellationToken, creationOptions, scheduler);
        }

        public Task<TResult> StartNew(Func<object, TResult> function, object state) { return Task.Factory.StartNew(function, state); }
    }
}

namespace System.Runtime.CompilerServices
{
    public interface IAsyncStateMachine
    {
        void MoveNext();
        void SetStateMachine(IAsyncStateMachine stateMachine);
    }

    public interface INotifyCompletion
    {
        void OnCompleted(Action continuation);
    }

    public interface ICriticalNotifyCompletion : INotifyCompletion
    {
        void UnsafeOnCompleted(Action continuation);
    }

    public struct TaskAwaiter : ICriticalNotifyCompletion
    {
        private readonly Task _task;

        internal TaskAwaiter(Task task) { _task = task; }

        public bool IsCompleted { get { return _task.IsCompleted; } }
        public void OnCompleted(Action continuation) { _task.AddContinuation(continuation); }
        public void UnsafeOnCompleted(Action continuation) { _task.AddContinuation(continuation); }
        public void GetResult() { _task.ThrowIfNotSuccess(); }
    }

    public struct TaskAwaiter<TResult> : ICriticalNotifyCompletion
    {
        private readonly Task<TResult> _task;

        internal TaskAwaiter(Task<TResult> task) { _task = task; }

        public bool IsCompleted { get { return _task.IsCompleted; } }
        public void OnCompleted(Action continuation) { _task.AddContinuation(continuation); }
        public void UnsafeOnCompleted(Action continuation) { _task.AddContinuation(continuation); }
        public TResult GetResult() { return _task.GetResultForAwait(); }
    }

    public struct ConfiguredTaskAwaitable
    {
        private readonly ConfiguredTaskAwaiter _awaiter;

        internal ConfiguredTaskAwaitable(Task task) { _awaiter = new ConfiguredTaskAwaiter(task); }

        public ConfiguredTaskAwaiter GetAwaiter() { return _awaiter; }

        public struct ConfiguredTaskAwaiter : ICriticalNotifyCompletion
        {
            private readonly Task _task;

            internal ConfiguredTaskAwaiter(Task task) { _task = task; }

            public bool IsCompleted { get { return _task.IsCompleted; } }
            public void OnCompleted(Action continuation) { _task.AddContinuation(continuation); }
            public void UnsafeOnCompleted(Action continuation) { _task.AddContinuation(continuation); }
            public void GetResult() { _task.ThrowIfNotSuccess(); }
        }
    }

    public struct ConfiguredTaskAwaitable<TResult>
    {
        private readonly ConfiguredTaskAwaiter _awaiter;

        internal ConfiguredTaskAwaitable(Task<TResult> task) { _awaiter = new ConfiguredTaskAwaiter(task); }

        public ConfiguredTaskAwaiter GetAwaiter() { return _awaiter; }

        public struct ConfiguredTaskAwaiter : ICriticalNotifyCompletion
        {
            private readonly Task<TResult> _task;

            internal ConfiguredTaskAwaiter(Task<TResult> task) { _task = task; }

            public bool IsCompleted { get { return _task.IsCompleted; } }
            public void OnCompleted(Action continuation) { _task.AddContinuation(continuation); }
            public void UnsafeOnCompleted(Action continuation) { _task.AddContinuation(continuation); }
            public TResult GetResult() { return _task.GetResultForAwait(); }
        }
    }

    public struct YieldAwaitable
    {
        public YieldAwaiter GetAwaiter() { return default(YieldAwaiter); }

        public struct YieldAwaiter : ICriticalNotifyCompletion
        {
            private static readonly WaitCallback s_run = RunAction;

            public bool IsCompleted { get { return false; } }
            public void OnCompleted(Action continuation) { ThreadPool.QueueUserWorkItem(s_run, continuation); }
            public void UnsafeOnCompleted(Action continuation) { ThreadPool.QueueUserWorkItem(s_run, continuation); }
            public void GetResult() { }

            private static void RunAction(object a) { ((Action)a)(); }
        }
    }

    // Partie commune des constructeurs de méthodes async : à la première
    // attente qui ne se termine pas tout de suite, la machine à états (une
    // structure en version optimisée) est copiée une fois dans un objet ;
    // la tâche est créée avant la copie pour que l'original et la copie
    // partagent la même.
    internal static class Async
    {
        internal static Action Continuation<TStateMachine>(ref IAsyncStateMachine box, ref TStateMachine stateMachine)
            where TStateMachine : IAsyncStateMachine
        {
            if (box == null)
            {
                IAsyncStateMachine copie = stateMachine;
                box = copie;
                copie.SetStateMachine(copie);
            }
            return new Relance(box).Run;
        }

        private sealed class Relance
        {
            private readonly IAsyncStateMachine _sm;
            public Relance(IAsyncStateMachine sm) { _sm = sm; }
            public void Run() { _sm.MoveNext(); }
        }
    }

    public struct AsyncTaskMethodBuilder<TResult>
    {
        private Task<TResult> _task;
        private IAsyncStateMachine _box;

        public static AsyncTaskMethodBuilder<TResult> Create() { return default(AsyncTaskMethodBuilder<TResult>); }

        public void Start<TStateMachine>(ref TStateMachine stateMachine) where TStateMachine : IAsyncStateMachine
        {
            if (stateMachine == null) throw new ArgumentNullException("stateMachine");
            stateMachine.MoveNext();
        }

        public void SetStateMachine(IAsyncStateMachine stateMachine)
        {
            if (stateMachine == null) throw new ArgumentNullException("stateMachine");
            _box = stateMachine;
        }

        public Task<TResult> Task
        {
            get
            {
                if (_task == null) _task = new Task<TResult>();
                return _task;
            }
        }

        public void AwaitOnCompleted<TAwaiter, TStateMachine>(ref TAwaiter awaiter, ref TStateMachine stateMachine)
            where TAwaiter : INotifyCompletion
            where TStateMachine : IAsyncStateMachine
        {
            if (_task == null) _task = new Task<TResult>();
            awaiter.OnCompleted(Async.Continuation(ref _box, ref stateMachine));
        }

        public void AwaitUnsafeOnCompleted<TAwaiter, TStateMachine>(ref TAwaiter awaiter, ref TStateMachine stateMachine)
            where TAwaiter : ICriticalNotifyCompletion
            where TStateMachine : IAsyncStateMachine
        {
            if (_task == null) _task = new Task<TResult>();
            awaiter.UnsafeOnCompleted(Async.Continuation(ref _box, ref stateMachine));
        }

        public void SetResult(TResult result)
        {
            if (_task == null) _task = new Task<TResult>();
            if (!_task.TrySetResult(result)) throw new InvalidOperationException("The task has already completed.");
        }

        public void SetException(Exception exception)
        {
            if (exception == null) throw new ArgumentNullException("exception");
            if (_task == null) _task = new Task<TResult>();
            OperationCanceledException oce = exception as OperationCanceledException;
            bool ok = oce != null ? _task.TrySetCanceled(oce.CancellationToken) : _task.TrySetException(exception);
            if (!ok) throw new InvalidOperationException("The task has already completed.");
        }
    }

    public struct AsyncTaskMethodBuilder
    {
        private AsyncTaskMethodBuilder<object> _builder;

        public static AsyncTaskMethodBuilder Create() { return default(AsyncTaskMethodBuilder); }

        public void Start<TStateMachine>(ref TStateMachine stateMachine) where TStateMachine : IAsyncStateMachine
        {
            if (stateMachine == null) throw new ArgumentNullException("stateMachine");
            stateMachine.MoveNext();
        }

        public void SetStateMachine(IAsyncStateMachine stateMachine) { _builder.SetStateMachine(stateMachine); }
        public Task Task { get { return _builder.Task; } }

        public void AwaitOnCompleted<TAwaiter, TStateMachine>(ref TAwaiter awaiter, ref TStateMachine stateMachine)
            where TAwaiter : INotifyCompletion
            where TStateMachine : IAsyncStateMachine
        {
            _builder.AwaitOnCompleted(ref awaiter, ref stateMachine);
        }

        public void AwaitUnsafeOnCompleted<TAwaiter, TStateMachine>(ref TAwaiter awaiter, ref TStateMachine stateMachine)
            where TAwaiter : ICriticalNotifyCompletion
            where TStateMachine : IAsyncStateMachine
        {
            _builder.AwaitUnsafeOnCompleted(ref awaiter, ref stateMachine);
        }

        public void SetResult() { _builder.SetResult(null); }
        public void SetException(Exception exception) { _builder.SetException(exception); }
    }

    public struct AsyncVoidMethodBuilder
    {
        private IAsyncStateMachine _box;

        public static AsyncVoidMethodBuilder Create() { return default(AsyncVoidMethodBuilder); }

        public void Start<TStateMachine>(ref TStateMachine stateMachine) where TStateMachine : IAsyncStateMachine
        {
            if (stateMachine == null) throw new ArgumentNullException("stateMachine");
            stateMachine.MoveNext();
        }

        public void SetStateMachine(IAsyncStateMachine stateMachine)
        {
            if (stateMachine == null) throw new ArgumentNullException("stateMachine");
            _box = stateMachine;
        }

        public void AwaitOnCompleted<TAwaiter, TStateMachine>(ref TAwaiter awaiter, ref TStateMachine stateMachine)
            where TAwaiter : INotifyCompletion
            where TStateMachine : IAsyncStateMachine
        {
            awaiter.OnCompleted(Async.Continuation(ref _box, ref stateMachine));
        }

        public void AwaitUnsafeOnCompleted<TAwaiter, TStateMachine>(ref TAwaiter awaiter, ref TStateMachine stateMachine)
            where TAwaiter : ICriticalNotifyCompletion
            where TStateMachine : IAsyncStateMachine
        {
            awaiter.UnsafeOnCompleted(Async.Continuation(ref _box, ref stateMachine));
        }

        public void SetResult() { }

        // Sans SynchronizationContext, le Framework relance l'exception sur
        // le réservoir de fils, ce qui termine le processus.
        public void SetException(Exception exception)
        {
            if (exception == null) throw new ArgumentNullException("exception");
            ThreadPool.QueueUserWorkItem(Relancer, exception);
        }

        private static void Relancer(object e)
        {
            throw (Exception)e;
        }
    }

    public static class RuntimeHelpers
    {
        // Copie les octets d'un champ à RVA (initialiseur de tableau) dans
        // le tableau : servi par le moteur (tableaux.rs).
        [MethodImpl(MethodImplOptions.InternalCall)]
        public static extern void InitializeArray(Array array, RuntimeFieldHandle fldHandle);

        public static int OffsetToStringData { get { return 8; } }
        public static void EnsureSufficientExecutionStack() { }
        public static void PrepareConstrainedRegions() { }
        public static void ProbeForSufficientStack() { }
    }

    // --- Attributs émis par les compilateurs ---

    [AttributeUsage(AttributeTargets.All, Inherited = true)]
    public sealed class CompilerGeneratedAttribute : Attribute { }

    [AttributeUsage(AttributeTargets.Method, Inherited = false, AllowMultiple = false)]
    public class StateMachineAttribute : Attribute
    {
        public StateMachineAttribute(Type stateMachineType) { StateMachineType = stateMachineType; }
        public Type StateMachineType { get; private set; }
    }

    [AttributeUsage(AttributeTargets.Method, Inherited = false, AllowMultiple = false)]
    public sealed class AsyncStateMachineAttribute : StateMachineAttribute
    {
        public AsyncStateMachineAttribute(Type stateMachineType) : base(stateMachineType) { }
    }

    [AttributeUsage(AttributeTargets.Method, Inherited = false, AllowMultiple = false)]
    public sealed class IteratorStateMachineAttribute : StateMachineAttribute
    {
        public IteratorStateMachineAttribute(Type stateMachineType) : base(stateMachineType) { }
    }

    [AttributeUsage(AttributeTargets.Assembly | AttributeTargets.Class | AttributeTargets.Method)]
    public sealed class ExtensionAttribute : Attribute { }

    [AttributeUsage(AttributeTargets.Assembly, Inherited = false, AllowMultiple = false)]
    public sealed class RuntimeCompatibilityAttribute : Attribute
    {
        public bool WrapNonExceptionThrows { get; set; }
    }

    [AttributeUsage(AttributeTargets.Assembly | AttributeTargets.Module | AttributeTargets.Class | AttributeTargets.Method)]
    public class CompilationRelaxationsAttribute : Attribute
    {
        public CompilationRelaxationsAttribute(int relaxations) { CompilationRelaxations = relaxations; }
        public int CompilationRelaxations { get; private set; }
    }

    [AttributeUsage(AttributeTargets.Assembly, AllowMultiple = true, Inherited = false)]
    public sealed class InternalsVisibleToAttribute : Attribute
    {
        public InternalsVisibleToAttribute(string assemblyName) { AssemblyName = assemblyName; }
        public string AssemblyName { get; private set; }
        public bool AllInternalsVisible { get; set; }
    }
}

namespace System.Diagnostics
{
    [AttributeUsage(AttributeTargets.Class | AttributeTargets.Struct | AttributeTargets.Method | AttributeTargets.Constructor, Inherited = false)]
    public sealed class DebuggerStepThroughAttribute : Attribute { }

    [AttributeUsage(AttributeTargets.Property | AttributeTargets.Method | AttributeTargets.Constructor, Inherited = false)]
    public sealed class DebuggerHiddenAttribute : Attribute { }

    [AttributeUsage(AttributeTargets.Class | AttributeTargets.Struct | AttributeTargets.Method | AttributeTargets.Constructor, Inherited = false)]
    public sealed class DebuggerNonUserCodeAttribute : Attribute { }

    public enum DebuggerBrowsableState
    {
        Never = 0,
        Collapsed = 2,
        RootHidden = 3,
    }

    [AttributeUsage(AttributeTargets.Field | AttributeTargets.Property, AllowMultiple = false)]
    public sealed class DebuggerBrowsableAttribute : Attribute
    {
        public DebuggerBrowsableAttribute(DebuggerBrowsableState state) { State = state; }
        public DebuggerBrowsableState State { get; private set; }
    }

    [AttributeUsage(AttributeTargets.Assembly | AttributeTargets.Class | AttributeTargets.Struct | AttributeTargets.Delegate | AttributeTargets.Enum | AttributeTargets.Field | AttributeTargets.Property, AllowMultiple = true)]
    public sealed class DebuggerDisplayAttribute : Attribute
    {
        public DebuggerDisplayAttribute(string value) { Value = value; }
        public string Value { get; private set; }
        public string Name { get; set; }
        public string Type { get; set; }
    }
}
