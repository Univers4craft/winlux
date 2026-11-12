// BCL Winlux : System.Diagnostics.Stopwatch (#1922). Licence GPL 3.
// Écrit d'après la documentation publique de l'API (« Stopwatch Class ») ;
// aucun code de Microsoft, de Mono ou de Wine.
//
// L'horloge est monotone, servie par le moteur (bcl.rs) en graduations de
// 100 ns : Frequency vaut donc 10 000 000, et une graduation de Stopwatch
// est une graduation de TimeSpan.

using System.Runtime.CompilerServices;

namespace System.Diagnostics
{
    public class Stopwatch
    {
        public static readonly long Frequency = 10000000;
        public static readonly bool IsHighResolution = true;

        private long _elapsed;
        private long _startTimestamp;
        private bool _isRunning;

        [MethodImpl(MethodImplOptions.InternalCall)]
        public static extern long GetTimestamp();

        public static Stopwatch StartNew()
        {
            Stopwatch s = new Stopwatch();
            s.Start();
            return s;
        }

        public void Start()
        {
            if (!_isRunning)
            {
                _startTimestamp = GetTimestamp();
                _isRunning = true;
            }
        }

        public void Stop()
        {
            if (_isRunning)
            {
                long elapsed = GetTimestamp() - _startTimestamp;
                _elapsed += elapsed;
                _isRunning = false;
                if (_elapsed < 0)
                    _elapsed = 0;
            }
        }

        public void Reset()
        {
            _elapsed = 0;
            _isRunning = false;
            _startTimestamp = 0;
        }

        public void Restart()
        {
            _elapsed = 0;
            _startTimestamp = GetTimestamp();
            _isRunning = true;
        }

        public bool IsRunning { get { return _isRunning; } }

        public long ElapsedTicks
        {
            get
            {
                long elapsed = _elapsed;
                if (_isRunning)
                    elapsed += GetTimestamp() - _startTimestamp;
                return elapsed;
            }
        }

        public TimeSpan Elapsed { get { return new TimeSpan(ElapsedTicks); } }

        public long ElapsedMilliseconds { get { return ElapsedTicks / TimeSpan.TicksPerMillisecond; } }
    }
}
