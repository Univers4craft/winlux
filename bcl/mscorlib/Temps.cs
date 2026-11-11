// BCL Winlux : System.TimeSpan (étape 7, #1686). Licence GPL 3.
// Écrit d'après la documentation publique de l'API (« TimeSpan Struct ») et
// ECMA-335 ; aucun code de Microsoft, de Mono ou de Wine.
//
// Une durée est un nombre signé de graduations de 100 ns. Les conversions
// depuis un double (FromMilliseconds, FromSeconds…) arrondissent à la
// milliseconde la plus proche, comme le documente le .NET Framework, et
// lèvent OverflowException hors de [MinValue, MaxValue] ou pour NaN.

namespace System
{
    public class SystemException : Exception
    {
        public SystemException() { }
        public SystemException(string message) : base(message) { }
    }

    public class ArithmeticException : SystemException
    {
        public ArithmeticException() { }
        public ArithmeticException(string message) : base(message) { }
    }

    public class OverflowException : ArithmeticException
    {
        public OverflowException() { }
        public OverflowException(string message) : base(message) { }
    }

    public class ArgumentException : SystemException
    {
        public ArgumentException() { }
        public ArgumentException(string message) : base(message) { }
    }

    public struct TimeSpan
    {
        public const long TicksPerMillisecond = 10000;
        public const long TicksPerSecond = TicksPerMillisecond * 1000;
        public const long TicksPerMinute = TicksPerSecond * 60;
        public const long TicksPerHour = TicksPerMinute * 60;
        public const long TicksPerDay = TicksPerHour * 24;

        private const long MaxMilliseconds = long.MaxValue / TicksPerMillisecond;
        private const long MinMilliseconds = long.MinValue / TicksPerMillisecond;

        public static readonly TimeSpan Zero = new TimeSpan(0);
        public static readonly TimeSpan MaxValue = new TimeSpan(long.MaxValue);
        public static readonly TimeSpan MinValue = new TimeSpan(long.MinValue);

        private long _ticks;

        public TimeSpan(long ticks)
        {
            _ticks = ticks;
        }

        public TimeSpan(int hours, int minutes, int seconds)
        {
            _ticks = FromParts(0, hours, minutes, seconds, 0);
        }

        public TimeSpan(int days, int hours, int minutes, int seconds)
        {
            _ticks = FromParts(days, hours, minutes, seconds, 0);
        }

        public TimeSpan(int days, int hours, int minutes, int seconds, int milliseconds)
        {
            _ticks = FromParts(days, hours, minutes, seconds, milliseconds);
        }

        private static long FromParts(int days, int hours, int minutes, int seconds, int milliseconds)
        {
            long ms = ((long)days * 86400 + (long)hours * 3600 + (long)minutes * 60 + seconds) * 1000
                + milliseconds;
            if (ms > MaxMilliseconds || ms < MinMilliseconds)
                throw new ArgumentException("TimeSpan overflowed because the duration is too long.");
            return ms * TicksPerMillisecond;
        }

        public long Ticks { get { return _ticks; } }
        public int Days { get { return (int)(_ticks / TicksPerDay); } }
        public int Hours { get { return (int)((_ticks / TicksPerHour) % 24); } }
        public int Minutes { get { return (int)((_ticks / TicksPerMinute) % 60); } }
        public int Seconds { get { return (int)((_ticks / TicksPerSecond) % 60); } }
        public int Milliseconds { get { return (int)((_ticks / TicksPerMillisecond) % 1000); } }

        public double TotalDays { get { return (double)_ticks / TicksPerDay; } }
        public double TotalHours { get { return (double)_ticks / TicksPerHour; } }
        public double TotalMinutes { get { return (double)_ticks / TicksPerMinute; } }
        public double TotalSeconds { get { return (double)_ticks / TicksPerSecond; } }

        public double TotalMilliseconds
        {
            get
            {
                double ms = (double)_ticks / TicksPerMillisecond;
                if (ms > MaxMilliseconds) return MaxMilliseconds;
                if (ms < MinMilliseconds) return MinMilliseconds;
                return ms;
            }
        }

        // value × scale millisecondes, arrondi à la milliseconde la plus proche.
        private static TimeSpan Interval(double value, int scale)
        {
            if (double.IsNaN(value))
                throw new ArgumentException("TimeSpan does not accept floating point Not-a-Number values.");
            double ms = value * scale + (value >= 0 ? 0.5 : -0.5);
            if (ms > MaxMilliseconds || ms < MinMilliseconds)
                throw new OverflowException("TimeSpan overflowed because the duration is too long.");
            return new TimeSpan((long)ms * TicksPerMillisecond);
        }

        public static TimeSpan FromDays(double value) { return Interval(value, 86400000); }
        public static TimeSpan FromHours(double value) { return Interval(value, 3600000); }
        public static TimeSpan FromMinutes(double value) { return Interval(value, 60000); }
        public static TimeSpan FromSeconds(double value) { return Interval(value, 1000); }
        public static TimeSpan FromMilliseconds(double value) { return Interval(value, 1); }
        public static TimeSpan FromTicks(long value) { return new TimeSpan(value); }

        public TimeSpan Add(TimeSpan ts)
        {
            long result = _ticks + ts._ticks;
            // Débordement : les deux opérandes ont le même signe et le résultat un autre.
            if ((_ticks >> 63) == (ts._ticks >> 63) && (_ticks >> 63) != (result >> 63))
                throw new OverflowException("TimeSpan overflowed because the duration is too long.");
            return new TimeSpan(result);
        }

        public TimeSpan Subtract(TimeSpan ts)
        {
            long result = _ticks - ts._ticks;
            if ((_ticks >> 63) != (ts._ticks >> 63) && (_ticks >> 63) != (result >> 63))
                throw new OverflowException("TimeSpan overflowed because the duration is too long.");
            return new TimeSpan(result);
        }

        public TimeSpan Negate()
        {
            if (_ticks == long.MinValue)
                throw new OverflowException("Negating the minimum value of a twos complement number is invalid.");
            return new TimeSpan(-_ticks);
        }

        public TimeSpan Duration()
        {
            if (_ticks == long.MinValue)
                throw new OverflowException("The duration cannot be returned for TimeSpan.MinValue because the absolute value of TimeSpan.MinValue exceeds the value of TimeSpan.MaxValue.");
            return new TimeSpan(_ticks >= 0 ? _ticks : -_ticks);
        }

        public static int Compare(TimeSpan t1, TimeSpan t2)
        {
            if (t1._ticks > t2._ticks) return 1;
            if (t1._ticks < t2._ticks) return -1;
            return 0;
        }

        public int CompareTo(TimeSpan value) { return Compare(this, value); }

        public int CompareTo(object value)
        {
            if (value == null) return 1;
            if (!(value is TimeSpan))
                throw new ArgumentException("Object must be of type TimeSpan.");
            return Compare(this, (TimeSpan)value);
        }

        public bool Equals(TimeSpan obj) { return _ticks == obj._ticks; }
        public static bool Equals(TimeSpan t1, TimeSpan t2) { return t1._ticks == t2._ticks; }

        public override bool Equals(object value)
        {
            return value is TimeSpan && _ticks == ((TimeSpan)value)._ticks;
        }

        public override int GetHashCode() { return (int)_ticks ^ (int)(_ticks >> 32); }

        public static TimeSpan operator -(TimeSpan t) { return t.Negate(); }
        public static TimeSpan operator +(TimeSpan t) { return t; }
        public static TimeSpan operator +(TimeSpan t1, TimeSpan t2) { return t1.Add(t2); }
        public static TimeSpan operator -(TimeSpan t1, TimeSpan t2) { return t1.Subtract(t2); }
        public static bool operator ==(TimeSpan t1, TimeSpan t2) { return t1._ticks == t2._ticks; }
        public static bool operator !=(TimeSpan t1, TimeSpan t2) { return t1._ticks != t2._ticks; }
        public static bool operator <(TimeSpan t1, TimeSpan t2) { return t1._ticks < t2._ticks; }
        public static bool operator <=(TimeSpan t1, TimeSpan t2) { return t1._ticks <= t2._ticks; }
        public static bool operator >(TimeSpan t1, TimeSpan t2) { return t1._ticks > t2._ticks; }
        public static bool operator >=(TimeSpan t1, TimeSpan t2) { return t1._ticks >= t2._ticks; }
    }
}
