// BCL Winlux : System.Lazy<T> et ce dont il a besoin (étape 7, #1701).
// Licence GPL 3. Écrit d'après la documentation publique de l'API
// (« Lazy<T> Class », « LazyThreadSafetyMode Enum ») et ECMA-335 ; aucun
// code de Microsoft, de Mono ou de Wine.

namespace System
{
    public delegate TResult Func<TResult>();

    public class InvalidOperationException : SystemException
    {
        public InvalidOperationException() { }
        public InvalidOperationException(string message) : base(message) { }
    }

    public class MissingMemberException : SystemException
    {
        public MissingMemberException() { }
        public MissingMemberException(string message) : base(message) { }
    }

    public class MissingMethodException : MissingMemberException
    {
        public MissingMethodException() { }
        public MissingMethodException(string message) : base(message) { }
    }

    public class ArgumentNullException : ArgumentException
    {
        public ArgumentNullException() { }
        public ArgumentNullException(string paramName) : base("Value cannot be null.") { }
    }

    // La valeur est calculée au premier accès à Value, une seule fois.
    // ExecutionAndPublication (le défaut) : un seul fil exécute la fabrique,
    // les autres attendent. PublicationOnly : plusieurs fils peuvent
    // l'exécuter, la première valeur rangée gagne. None : aucun verrou.
    // Une exception de la fabrique est mémorisée et relancée à chaque accès,
    // sauf en PublicationOnly où l'accès suivant réessaie.
    public class Lazy<T>
    {
        private Func<T> _factory;
        private readonly System.Threading.LazyThreadSafetyMode _mode;
        private readonly object _verrou;
        private bool _cree;
        private T _valeur;
        private Exception _erreur;
        private bool _enCours;

        public Lazy() : this(null, System.Threading.LazyThreadSafetyMode.ExecutionAndPublication) { }

        public Lazy(bool isThreadSafe)
            : this(null, isThreadSafe ? System.Threading.LazyThreadSafetyMode.ExecutionAndPublication
                                      : System.Threading.LazyThreadSafetyMode.None) { }

        public Lazy(System.Threading.LazyThreadSafetyMode mode) : this(null, mode) { }

        public Lazy(Func<T> valueFactory)
            : this(Requis(valueFactory), System.Threading.LazyThreadSafetyMode.ExecutionAndPublication) { }

        public Lazy(Func<T> valueFactory, bool isThreadSafe)
            : this(Requis(valueFactory), isThreadSafe ? System.Threading.LazyThreadSafetyMode.ExecutionAndPublication
                                                      : System.Threading.LazyThreadSafetyMode.None) { }

        public Lazy(Func<T> valueFactory, System.Threading.LazyThreadSafetyMode mode)
        {
            _factory = valueFactory;
            _mode = mode;
            if (mode != System.Threading.LazyThreadSafetyMode.None)
                _verrou = new object();
        }

        private static Func<T> Requis(Func<T> valueFactory)
        {
            if (valueFactory == null)
                throw new ArgumentNullException("valueFactory");
            return valueFactory;
        }

        public bool IsValueCreated { get { return _cree; } }

        public T Value
        {
            get
            {
                if (_cree)
                    return _valeur;
                if (_mode == System.Threading.LazyThreadSafetyMode.PublicationOnly)
                {
                    T v = Fabriquer();
                    lock (_verrou)
                    {
                        if (!_cree)
                        {
                            _valeur = v;
                            _cree = true;
                        }
                    }
                    return _valeur;
                }
                if (_verrou == null)
                    return Creer();
                lock (_verrou)
                {
                    return Creer();
                }
            }
        }

        private T Creer()
        {
            if (_cree)
                return _valeur;
            if (_erreur != null)
                throw _erreur;
            if (_enCours)
                throw new InvalidOperationException("ValueFactory attempted to access the Value property of this instance.");
            _enCours = true;
            try
            {
                _valeur = Fabriquer();
                _cree = true;
                _factory = null;
            }
            catch (Exception e)
            {
                _erreur = e;
                throw;
            }
            finally
            {
                _enCours = false;
            }
            return _valeur;
        }

        private T Fabriquer()
        {
            if (_factory == null)
                // Sans fabrique, .NET appelle le constructeur sans paramètre
                // de T (Activator.CreateInstance<T>), que le moteur ne sait
                // pas encore faire : on le dit plutôt que de rendre null.
                throw new MissingMethodException("Lazy<T> sans fabrique : Activator.CreateInstance<T> n'est pas encore pris en charge par Winlux.");
            return _factory();
        }

        public override string ToString()
        {
            return _cree ? (_valeur == null ? "" : _valeur.ToString()) : "Value is not created.";
        }
    }
}

namespace System.Threading
{
    public enum LazyThreadSafetyMode
    {
        None = 0,
        PublicationOnly = 1,
        ExecutionAndPublication = 2,
    }

    public static class Monitor
    {
        // Verrous servis par le moteur (runtime/src/clr/threads.rs).
        [System.Runtime.CompilerServices.MethodImpl(System.Runtime.CompilerServices.MethodImplOptions.InternalCall)]
        public static extern void Enter(object obj);

        [System.Runtime.CompilerServices.MethodImpl(System.Runtime.CompilerServices.MethodImplOptions.InternalCall)]
        public static extern void Enter(object obj, ref bool lockTaken);

        [System.Runtime.CompilerServices.MethodImpl(System.Runtime.CompilerServices.MethodImplOptions.InternalCall)]
        public static extern void Exit(object obj);

        [System.Runtime.CompilerServices.MethodImpl(System.Runtime.CompilerServices.MethodImplOptions.InternalCall)]
        public static extern bool TryEnter(object obj);

        [System.Runtime.CompilerServices.MethodImpl(System.Runtime.CompilerServices.MethodImplOptions.InternalCall)]
        public static extern bool TryEnter(object obj, int millisecondsTimeout);

        [System.Runtime.CompilerServices.MethodImpl(System.Runtime.CompilerServices.MethodImplOptions.InternalCall)]
        public static extern void TryEnter(object obj, ref bool lockTaken);

        [System.Runtime.CompilerServices.MethodImpl(System.Runtime.CompilerServices.MethodImplOptions.InternalCall)]
        public static extern void TryEnter(object obj, int millisecondsTimeout, ref bool lockTaken);

        public static bool TryEnter(object obj, TimeSpan timeout) { return TryEnter(obj, (int)timeout.TotalMilliseconds); }

        [System.Runtime.CompilerServices.MethodImpl(System.Runtime.CompilerServices.MethodImplOptions.InternalCall)]
        public static extern bool IsEntered(object obj);

        [System.Runtime.CompilerServices.MethodImpl(System.Runtime.CompilerServices.MethodImplOptions.InternalCall)]
        public static extern bool Wait(object obj);

        [System.Runtime.CompilerServices.MethodImpl(System.Runtime.CompilerServices.MethodImplOptions.InternalCall)]
        public static extern bool Wait(object obj, int millisecondsTimeout);

        public static bool Wait(object obj, TimeSpan timeout) { return Wait(obj, (int)timeout.TotalMilliseconds); }
        public static bool Wait(object obj, int millisecondsTimeout, bool exitContext) { return Wait(obj, millisecondsTimeout); }

        [System.Runtime.CompilerServices.MethodImpl(System.Runtime.CompilerServices.MethodImplOptions.InternalCall)]
        public static extern void Pulse(object obj);

        [System.Runtime.CompilerServices.MethodImpl(System.Runtime.CompilerServices.MethodImplOptions.InternalCall)]
        public static extern void PulseAll(object obj);
    }
}
