// BCL Winlux : DateTime, DateTimeOffset, DateTimeKind, DayOfWeek et
// TimeSpan.ToString (#1963). Licence GPL 3.
// Écrit d'après la documentation publique de l'API (« DateTime Struct »,
// « Standard date and time format strings », « Custom date and time format
// strings », « DateTimeOffset Struct ») ; aucun code de Microsoft, de Mono
// ou de Wine. Le calendrier est le grégorien proleptique, compté en jours
// depuis le 1er janvier de l'an 1 par cycles de 400 ans.
//
// Les textes suivent la culture invariante (« 11/11/2026 20:30:00 »).
// L'horloge et le fuseau horaire viennent du moteur (bcl.rs).

using System.Runtime.CompilerServices;
using System.Text;

namespace System
{
    internal static class Horloge
    {
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern long UtcNowTicks();

        // Décalage de l'heure locale à l'instant UTC donné.
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern long LocalOffsetTicks(long utcTicks);

        // Décalage d'une heure locale : celui de l'instant UTC qu'elle
        // désigne, cherché en deux temps autour d'un changement d'heure.
        internal static long OffsetOfLocal(long localTicks)
        {
            long guess = LocalOffsetTicks(localTicks);
            return LocalOffsetTicks(localTicks - guess);
        }
    }

    public enum DateTimeKind
    {
        Unspecified = 0,
        Utc = 1,
        Local = 2,
    }

    public enum DayOfWeek
    {
        Sunday = 0,
        Monday = 1,
        Tuesday = 2,
        Wednesday = 3,
        Thursday = 4,
        Friday = 5,
        Saturday = 6,
    }

    public partial struct TimeSpan : IFormattable
    {
        // Format « c » : [-][j.]hh:mm:ss[.fffffff].
        public override string ToString()
        {
            long t = _ticks;
            StringBuilder sb = new StringBuilder();
            ulong u = t < 0 ? 0UL - (ulong)t : (ulong)t;
            if (t < 0)
                sb.Append('-');
            ulong days = u / (ulong)TicksPerDay;
            if (days > 0)
                sb.Append(days.ToString()).Append('.');
            sb.Append(((u / (ulong)TicksPerHour) % 24).ToString("D2")).Append(':');
            sb.Append(((u / (ulong)TicksPerMinute) % 60).ToString("D2")).Append(':');
            sb.Append(((u / (ulong)TicksPerSecond) % 60).ToString("D2"));
            ulong frac = u % (ulong)TicksPerSecond;
            if (frac > 0)
                sb.Append('.').Append(frac.ToString("D7"));
            return sb.ToString();
        }

        public string ToString(string format)
        {
            return ToString();
        }

        public string ToString(string format, IFormatProvider formatProvider)
        {
            return ToString();
        }
    }

    public struct DateTime : IComparable, IComparable<DateTime>, IEquatable<DateTime>, IFormattable
    {
        private const long TicksPerDay = TimeSpan.TicksPerDay;
        private const long MaxTicks = 3652059L * TicksPerDay - 1;
        internal const long UnixEpochTicks = 621355968000000000L;
        private const long FileTimeEpochTicks = 504911232000000000L;

        public static readonly DateTime MinValue = new DateTime(0L, DateTimeKind.Unspecified);
        public static readonly DateTime MaxValue = new DateTime(MaxTicks, DateTimeKind.Unspecified);

        private static readonly string[] MonthNames = { "January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December" };
        private static readonly string[] DayNames = { "Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday" };

        private readonly long _ticks;
        private readonly DateTimeKind _kind;

        public DateTime(long ticks) : this(ticks, DateTimeKind.Unspecified) { }

        public DateTime(long ticks, DateTimeKind kind)
        {
            if (ticks < 0 || ticks > MaxTicks)
                throw new ArgumentOutOfRangeException("ticks");
            _ticks = ticks;
            _kind = kind;
        }

        public DateTime(int year, int month, int day)
            : this(DateToTicks(year, month, day), DateTimeKind.Unspecified) { }

        public DateTime(int year, int month, int day, int hour, int minute, int second)
            : this(year, month, day, hour, minute, second, 0, DateTimeKind.Unspecified) { }

        public DateTime(int year, int month, int day, int hour, int minute, int second, DateTimeKind kind)
            : this(year, month, day, hour, minute, second, 0, kind) { }

        public DateTime(int year, int month, int day, int hour, int minute, int second, int millisecond)
            : this(year, month, day, hour, minute, second, millisecond, DateTimeKind.Unspecified) { }

        public DateTime(int year, int month, int day, int hour, int minute, int second, int millisecond, DateTimeKind kind)
        {
            if (hour < 0 || hour > 23 || minute < 0 || minute > 59 || second < 0 || second > 59)
                throw new ArgumentOutOfRangeException(null, "Hour, Minute, and Second parameters describe an un-representable DateTime.");
            if (millisecond < 0 || millisecond > 999)
                throw new ArgumentOutOfRangeException("millisecond");
            _ticks = DateToTicks(year, month, day) + hour * TimeSpan.TicksPerHour + minute * TimeSpan.TicksPerMinute
                + second * TimeSpan.TicksPerSecond + millisecond * TimeSpan.TicksPerMillisecond;
            _kind = kind;
        }

        // ── Calendrier ──

        public static bool IsLeapYear(int year)
        {
            if (year < 1 || year > 9999)
                throw new ArgumentOutOfRangeException("year");
            return year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
        }

        public static int DaysInMonth(int year, int month)
        {
            if (month < 1 || month > 12)
                throw new ArgumentOutOfRangeException("month");
            if (month == 2)
                return IsLeapYear(year) ? 29 : 28;
            return month == 4 || month == 6 || month == 9 || month == 11 ? 30 : 31;
        }

        // Jours depuis 0001-01-01. L'année commence en mars dans le calcul,
        // pour que le 29 février tombe en fin d'année.
        private static long DaysFromCivil(int year, int month, int day)
        {
            long y = month <= 2 ? year - 1 : year;
            long era = y / 400;
            long yoe = y - era * 400;
            long mp = month > 2 ? month - 3 : month + 9;
            long doy = (153 * mp + 2) / 5 + day - 1;
            long doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
            return era * 146097 + doe - 306;
        }

        private static void CivilFromDays(long days, out int year, out int month, out int day)
        {
            long z = days + 306;
            long era = z / 146097;
            long doe = z - era * 146097;
            long yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
            long doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
            long mp = (5 * doy + 2) / 153;
            day = (int)(doy - (153 * mp + 2) / 5 + 1);
            month = (int)(mp < 10 ? mp + 3 : mp - 9);
            year = (int)(yoe + era * 400 + (month <= 2 ? 1 : 0));
        }

        private static long DateToTicks(int year, int month, int day)
        {
            if (year < 1 || year > 9999 || month < 1 || month > 12)
                throw new ArgumentOutOfRangeException(null, "Year, Month, and Day parameters describe an un-representable DateTime.");
            if (day < 1 || day > DaysInMonth(year, month))
                throw new ArgumentOutOfRangeException(null, "Year, Month, and Day parameters describe an un-representable DateTime.");
            return DaysFromCivil(year, month, day) * TicksPerDay;
        }

        // ── Propriétés ──

        public long Ticks { get { return _ticks; } }
        public DateTimeKind Kind { get { return _kind; } }

        public int Year { get { int y, m, d; CivilFromDays(_ticks / TicksPerDay, out y, out m, out d); return y; } }
        public int Month { get { int y, m, d; CivilFromDays(_ticks / TicksPerDay, out y, out m, out d); return m; } }
        public int Day { get { int y, m, d; CivilFromDays(_ticks / TicksPerDay, out y, out m, out d); return d; } }
        public int Hour { get { return (int)((_ticks / TimeSpan.TicksPerHour) % 24); } }
        public int Minute { get { return (int)((_ticks / TimeSpan.TicksPerMinute) % 60); } }
        public int Second { get { return (int)((_ticks / TimeSpan.TicksPerSecond) % 60); } }
        public int Millisecond { get { return (int)((_ticks / TimeSpan.TicksPerMillisecond) % 1000); } }
        public DayOfWeek DayOfWeek { get { return (DayOfWeek)((_ticks / TicksPerDay + 1) % 7); } }
        public int DayOfYear { get { return (int)(_ticks / TicksPerDay - DaysFromCivil(Year, 1, 1)) + 1; } }
        public DateTime Date { get { return new DateTime(_ticks - _ticks % TicksPerDay, _kind); } }
        public TimeSpan TimeOfDay { get { return new TimeSpan(_ticks % TicksPerDay); } }

        public static DateTime UtcNow { get { return new DateTime(Horloge.UtcNowTicks(), DateTimeKind.Utc); } }

        public static DateTime Now
        {
            get
            {
                long utc = Horloge.UtcNowTicks();
                return new DateTime(utc + Horloge.LocalOffsetTicks(utc), DateTimeKind.Local);
            }
        }

        public static DateTime Today { get { return Now.Date; } }

        public static DateTime SpecifyKind(DateTime value, DateTimeKind kind)
        {
            return new DateTime(value._ticks, kind);
        }

        // ── Fuseaux ──

        public DateTime ToUniversalTime()
        {
            if (_kind == DateTimeKind.Utc)
                return this;
            return new DateTime(Clamp(_ticks - Horloge.OffsetOfLocal(_ticks)), DateTimeKind.Utc);
        }

        public DateTime ToLocalTime()
        {
            if (_kind == DateTimeKind.Local)
                return this;
            return new DateTime(Clamp(_ticks + Horloge.LocalOffsetTicks(_ticks)), DateTimeKind.Local);
        }

        private static long Clamp(long ticks)
        {
            return ticks < 0 ? 0 : (ticks > MaxTicks ? MaxTicks : ticks);
        }

        public long ToFileTimeUtc()
        {
            long t = ToUniversalTime()._ticks - FileTimeEpochTicks;
            if (t < 0)
                throw new ArgumentOutOfRangeException(null, "Not a valid Win32 FileTime.");
            return t;
        }

        public long ToFileTime() { return ToFileTimeUtc(); }

        public static DateTime FromFileTimeUtc(long fileTime)
        {
            if (fileTime < 0 || fileTime > MaxTicks - FileTimeEpochTicks)
                throw new ArgumentOutOfRangeException("fileTime");
            return new DateTime(fileTime + FileTimeEpochTicks, DateTimeKind.Utc);
        }

        public static DateTime FromFileTime(long fileTime) { return FromFileTimeUtc(fileTime).ToLocalTime(); }

        public long ToBinary()
        {
            return _ticks | ((long)_kind << 62);
        }

        public static DateTime FromBinary(long dateData)
        {
            return new DateTime(dateData & 0x3FFFFFFFFFFFFFFFL, (DateTimeKind)((ulong)dateData >> 62));
        }

        public static DateTime FromOADate(double d)
        {
            // Jours depuis le 30 décembre 1899.
            long ms = (long)(d * 86400000.0 + (d >= 0 ? 0.5 : -0.5));
            return new DateTime(DaysFromCivil(1899, 12, 30) * TicksPerDay + ms * TimeSpan.TicksPerMillisecond);
        }

        // ── Calcul ──

        public DateTime Add(TimeSpan value) { return AddTicks(value.Ticks); }

        public DateTime AddTicks(long value)
        {
            long t = _ticks + value;
            if (t < 0 || t > MaxTicks)
                throw new ArgumentOutOfRangeException("value", "The added or subtracted value results in an un-representable DateTime.");
            return new DateTime(t, _kind);
        }

        private DateTime AddUnits(double value, long scale)
        {
            double ms = value * scale + (value >= 0 ? 0.5 : -0.5);
            if (ms <= -315537897600000.0 || ms >= 315537897600000.0)
                throw new ArgumentOutOfRangeException("value");
            return AddTicks((long)ms * TimeSpan.TicksPerMillisecond);
        }

        public DateTime AddMilliseconds(double value) { return AddUnits(value, 1); }
        public DateTime AddSeconds(double value) { return AddUnits(value, 1000); }
        public DateTime AddMinutes(double value) { return AddUnits(value, 60000); }
        public DateTime AddHours(double value) { return AddUnits(value, 3600000); }
        public DateTime AddDays(double value) { return AddUnits(value, 86400000); }

        public DateTime AddMonths(int months)
        {
            if (months < -120000 || months > 120000)
                throw new ArgumentOutOfRangeException("months");
            int y, m, d;
            CivilFromDays(_ticks / TicksPerDay, out y, out m, out d);
            int i = m - 1 + months;
            if (i >= 0)
            {
                m = i % 12 + 1;
                y += i / 12;
            }
            else
            {
                m = 12 + (i + 1) % 12;
                y += (i - 11) / 12;
            }
            if (y < 1 || y > 9999)
                throw new ArgumentOutOfRangeException("months");
            int dim = DaysInMonth(y, m);
            if (d > dim)
                d = dim;
            return new DateTime(DaysFromCivil(y, m, d) * TicksPerDay + _ticks % TicksPerDay, _kind);
        }

        public DateTime AddYears(int value)
        {
            if (value < -10000 || value > 10000)
                throw new ArgumentOutOfRangeException("value");
            return AddMonths(value * 12);
        }

        public TimeSpan Subtract(DateTime value) { return new TimeSpan(_ticks - value._ticks); }
        public DateTime Subtract(TimeSpan value) { return AddTicks(-value.Ticks); }

        public static DateTime operator +(DateTime d, TimeSpan t) { return d.AddTicks(t.Ticks); }
        public static DateTime operator -(DateTime d, TimeSpan t) { return d.AddTicks(-t.Ticks); }
        public static TimeSpan operator -(DateTime d1, DateTime d2) { return new TimeSpan(d1._ticks - d2._ticks); }
        public static bool operator ==(DateTime d1, DateTime d2) { return d1._ticks == d2._ticks; }
        public static bool operator !=(DateTime d1, DateTime d2) { return d1._ticks != d2._ticks; }
        public static bool operator <(DateTime t1, DateTime t2) { return t1._ticks < t2._ticks; }
        public static bool operator <=(DateTime t1, DateTime t2) { return t1._ticks <= t2._ticks; }
        public static bool operator >(DateTime t1, DateTime t2) { return t1._ticks > t2._ticks; }
        public static bool operator >=(DateTime t1, DateTime t2) { return t1._ticks >= t2._ticks; }

        public static int Compare(DateTime t1, DateTime t2)
        {
            return t1._ticks < t2._ticks ? -1 : (t1._ticks > t2._ticks ? 1 : 0);
        }

        public int CompareTo(DateTime value) { return Compare(this, value); }

        public int CompareTo(object value)
        {
            if (value == null)
                return 1;
            if (!(value is DateTime))
                throw new ArgumentException("Object must be of type DateTime.");
            return Compare(this, (DateTime)value);
        }

        public override bool Equals(object value)
        {
            return value is DateTime && ((DateTime)value)._ticks == _ticks;
        }

        public bool Equals(DateTime value) { return _ticks == value._ticks; }
        public static bool Equals(DateTime t1, DateTime t2) { return t1._ticks == t2._ticks; }
        public override int GetHashCode() { return (int)_ticks ^ (int)(_ticks >> 32); }

        // ── Texte ──

        public override string ToString() { return ToString("G"); }
        public string ToString(IFormatProvider provider) { return ToString("G"); }
        public string ToString(string format, IFormatProvider provider) { return ToString(format); }
        public string ToShortDateString() { return ToString("d"); }
        public string ToShortTimeString() { return ToString("t"); }
        public string ToLongDateString() { return ToString("D"); }
        public string ToLongTimeString() { return ToString("T"); }

        public string ToString(string format)
        {
            return Format(this, format, null);
        }

        private static string Expand(string format, bool hasOffset)
        {
            if ((object)format == null || format.Length == 0)
                return "MM/dd/yyyy HH:mm:ss";
            if (format.Length != 1)
                return format;
            switch (format[0])
            {
                case 'd': return "MM/dd/yyyy";
                case 'D': return "dddd, dd MMMM yyyy";
                case 'f': return "dddd, dd MMMM yyyy HH:mm";
                case 'F': return "dddd, dd MMMM yyyy HH:mm:ss";
                case 'g': return "MM/dd/yyyy HH:mm";
                case 'G': return hasOffset ? "MM/dd/yyyy HH:mm:ss zzz" : "MM/dd/yyyy HH:mm:ss";
                case 'm':
                case 'M': return "MMMM dd";
                case 'o':
                case 'O': return hasOffset ? "yyyy'-'MM'-'dd'T'HH':'mm':'ss.fffffffzzz" : "yyyy'-'MM'-'dd'T'HH':'mm':'ss.fffffffK";
                case 'r':
                case 'R': return "ddd, dd MMM yyyy HH':'mm':'ss 'GMT'";
                case 's': return "yyyy'-'MM'-'dd'T'HH':'mm':'ss";
                case 't': return "HH:mm";
                case 'T': return "HH:mm:ss";
                case 'u': return "yyyy'-'MM'-'dd HH':'mm':'ss'Z'";
                case 'y':
                case 'Y': return "yyyy MMMM";
            }
            throw new FormatException("Input string was not in a correct format.");
        }

        private static int Repeat(string format, int i)
        {
            int n = 1;
            while (i + n < format.Length && format[i + n] == format[i])
                n++;
            return n;
        }

        private static string Pad(long value, int width)
        {
            return value.ToString(string.Concat("D", width.ToString()));
        }

        private static string OffsetText(long ticks, int n)
        {
            bool neg = ticks < 0;
            long t = neg ? -ticks : ticks;
            int h = (int)(t / TimeSpan.TicksPerHour);
            int m = (int)((t / TimeSpan.TicksPerMinute) % 60);
            string sign = neg ? "-" : "+";
            if (n == 1)
                return string.Concat(sign, h.ToString());
            if (n == 2)
                return string.Concat(sign, Pad(h, 2));
            return string.Concat(sign, Pad(h, 2), ":", Pad(m, 2));
        }

        // `offset` : décalage d'un DateTimeOffset (null pour un DateTime).
        internal static string Format(DateTime dt, string format, object offset)
        {
            string f = Expand(format, offset != null);
            if (format == "r" || format == "R" || format == "u")
            {
                if (offset != null)
                    dt = new DateTime(dt._ticks - (long)offset, DateTimeKind.Utc);
            }
            int y, mo, d;
            CivilFromDays(dt._ticks / TicksPerDay, out y, out mo, out d);
            StringBuilder sb = new StringBuilder();
            int i = 0;
            while (i < f.Length)
            {
                char c = f[i];
                int n = Repeat(f, i);
                switch (c)
                {
                    case 'y':
                        sb.Append(n <= 2 ? Pad(y % 100, n) : Pad(y, n));
                        i += n;
                        continue;
                    case 'M':
                        sb.Append(n <= 2 ? Pad(mo, n) : (n == 3 ? MonthNames[mo - 1].Substring(0, 3) : MonthNames[mo - 1]));
                        i += n;
                        continue;
                    case 'd':
                        sb.Append(n <= 2 ? Pad(d, n) : (n == 3 ? DayNames[(int)dt.DayOfWeek].Substring(0, 3) : DayNames[(int)dt.DayOfWeek]));
                        i += n;
                        continue;
                    case 'H':
                        sb.Append(Pad(dt.Hour, n > 2 ? 2 : n));
                        i += n;
                        continue;
                    case 'h':
                        int h12 = dt.Hour % 12;
                        sb.Append(Pad(h12 == 0 ? 12 : h12, n > 2 ? 2 : n));
                        i += n;
                        continue;
                    case 'm':
                        sb.Append(Pad(dt.Minute, n > 2 ? 2 : n));
                        i += n;
                        continue;
                    case 's':
                        sb.Append(Pad(dt.Second, n > 2 ? 2 : n));
                        i += n;
                        continue;
                    case 'f':
                    case 'F':
                        {
                            int digits = n > 7 ? 7 : n;
                            long frac = dt._ticks % TimeSpan.TicksPerSecond;
                            for (int k = digits; k < 7; k++)
                                frac /= 10;
                            string text = Pad(frac, digits);
                            if (c == 'F')
                                text = text.TrimEnd('0');
                            if (c == 'F' && text.Length == 0 && sb.Length > 0 && sb[sb.Length - 1] == '.')
                                sb.Length = sb.Length - 1;
                            sb.Append(text);
                            i += n;
                            continue;
                        }
                    case 't':
                        string ampm = dt.Hour < 12 ? "AM" : "PM";
                        sb.Append(n == 1 ? ampm.Substring(0, 1) : ampm);
                        i += n;
                        continue;
                    case 'z':
                        {
                            long off = offset != null ? (long)offset
                                : (dt._kind == DateTimeKind.Utc ? 0 : Horloge.OffsetOfLocal(dt._ticks));
                            sb.Append(OffsetText(off, n > 3 ? 3 : n));
                            i += n;
                            continue;
                        }
                    case 'K':
                        if (offset != null)
                            sb.Append(OffsetText((long)offset, 3));
                        else if (dt._kind == DateTimeKind.Utc)
                            sb.Append('Z');
                        else if (dt._kind == DateTimeKind.Local)
                            sb.Append(OffsetText(Horloge.OffsetOfLocal(dt._ticks), 3));
                        i += n;
                        continue;
                    case '\'':
                    case '"':
                        {
                            int end = f.IndexOf(c, i + 1);
                            if (end < 0)
                                throw new FormatException("Cannot find a matching quote character for the character '\''.");
                            sb.Append(f.Substring(i + 1, end - i - 1));
                            i = end + 1;
                            continue;
                        }
                    case '\\':
                        if (i + 1 < f.Length)
                            sb.Append(f[i + 1]);
                        i += 2;
                        continue;
                    case '%':
                        i++;
                        continue;
                    default:
                        sb.Append(c);
                        i++;
                        continue;
                }
            }
            return sb.ToString();
        }

        // ── Lecture ──

        public static DateTime Parse(string s)
        {
            if ((object)s == null)
                throw new ArgumentNullException("s");
            DateTime result;
            if (!TryParse(s, out result))
                throw new FormatException(string.Concat("String '", s, "' was not recognized as a valid DateTime."));
            return result;
        }

        public static DateTime Parse(string s, IFormatProvider provider) { return Parse(s); }

        public static bool TryParse(string s, out DateTime result)
        {
            long offset;
            bool hasOffset;
            return TryParseCore(s, out result, out offset, out hasOffset);
        }

        private static bool ReadNumber(string s, ref int i, int min, int max, out int value)
        {
            value = 0;
            int n = 0;
            while (i < s.Length && n < max && s[i] >= '0' && s[i] <= '9')
            {
                value = value * 10 + (s[i] - '0');
                i++;
                n++;
            }
            return n >= min;
        }

        // ISO 8601 (« 2026-11-11T20:30:00.123Z », « 2026-11-11 20:30 ») et
        // forme invariante (« 11/11/2026 20:30:00 »). Une heure suivie de Z
        // ou d'un décalage donne une heure locale, comme DateTime.Parse.
        internal static bool TryParseCore(string s, out DateTime result, out long offset, out bool hasOffset)
        {
            result = MinValue;
            offset = 0;
            hasOffset = false;
            if ((object)s == null)
                return false;
            s = s.Trim();
            int i = 0;
            int y, mo, d;
            if (s.Length >= 8 && s.IndexOf('-') == 4)
            {
                if (!ReadNumber(s, ref i, 4, 4, out y) || i >= s.Length || s[i++] != '-'
                    || !ReadNumber(s, ref i, 1, 2, out mo) || i >= s.Length || s[i++] != '-'
                    || !ReadNumber(s, ref i, 1, 2, out d))
                    return false;
            }
            else
            {
                if (!ReadNumber(s, ref i, 1, 2, out mo) || i >= s.Length || s[i++] != '/'
                    || !ReadNumber(s, ref i, 1, 2, out d) || i >= s.Length || s[i++] != '/'
                    || !ReadNumber(s, ref i, 4, 4, out y))
                    return false;
            }
            if (y < 1 || mo < 1 || mo > 12 || d < 1 || d > DaysInMonth(y, mo))
                return false;
            int h = 0, mi = 0, se = 0;
            long frac = 0;
            if (i < s.Length && (s[i] == 'T' || s[i] == ' '))
            {
                i++;
                if (!ReadNumber(s, ref i, 1, 2, out h) || i >= s.Length || s[i++] != ':' || !ReadNumber(s, ref i, 2, 2, out mi))
                    return false;
                if (i < s.Length && s[i] == ':')
                {
                    i++;
                    if (!ReadNumber(s, ref i, 2, 2, out se))
                        return false;
                    if (i < s.Length && s[i] == '.')
                    {
                        i++;
                        int digits = 0;
                        while (i < s.Length && s[i] >= '0' && s[i] <= '9')
                        {
                            if (digits < 7)
                            {
                                frac = frac * 10 + (s[i] - '0');
                                digits++;
                            }
                            i++;
                        }
                        for (; digits < 7; digits++)
                            frac *= 10;
                    }
                }
                if (i < s.Length && (s[i] == ' ') && s.Length - i >= 3)
                {
                    string ampm = s.Substring(i + 1).ToUpperInvariant();
                    if (ampm == "PM" && h < 12) { h += 12; i = s.Length; }
                    else if (ampm == "AM") { if (h == 12) h = 0; i = s.Length; }
                }
            }
            if (h > 23 || mi > 59 || se > 59)
                return false;
            long ticks = DaysFromCivil(y, mo, d) * TicksPerDay + h * TimeSpan.TicksPerHour
                + mi * TimeSpan.TicksPerMinute + se * TimeSpan.TicksPerSecond + frac;
            if (i < s.Length && s[i] == 'Z')
            {
                hasOffset = true;
                i++;
            }
            else if (i < s.Length && (s[i] == '+' || s[i] == '-'))
            {
                bool neg = s[i] == '-';
                i++;
                int oh, om = 0;
                if (!ReadNumber(s, ref i, 2, 2, out oh))
                    return false;
                if (i < s.Length && s[i] == ':')
                    i++;
                ReadNumber(s, ref i, 0, 2, out om);
                offset = (oh * TimeSpan.TicksPerHour + om * TimeSpan.TicksPerMinute) * (neg ? -1 : 1);
                hasOffset = true;
            }
            if (i != s.Length)
                return false;
            if (hasOffset)
            {
                long utc = ticks - offset;
                if (utc < 0 || utc > MaxTicks)
                    return false;
                result = new DateTime(utc + Horloge.LocalOffsetTicks(utc), DateTimeKind.Local);
                // `result` porte l'heure locale ; le décalage d'origine reste
                // dans `offset` pour DateTimeOffset.
                return true;
            }
            result = new DateTime(ticks, DateTimeKind.Unspecified);
            return true;
        }
    }

    public struct DateTimeOffset : IComparable, IComparable<DateTimeOffset>, IEquatable<DateTimeOffset>, IFormattable
    {
        private readonly DateTime _local;
        private readonly long _offset;

        public static readonly DateTimeOffset MinValue = new DateTimeOffset(0L, TimeSpan.Zero);
        public static readonly DateTimeOffset MaxValue = new DateTimeOffset(DateTime.MaxValue.Ticks, TimeSpan.Zero);

        public DateTimeOffset(long ticks, TimeSpan offset)
        {
            CheckOffset(offset.Ticks);
            _local = new DateTime(ticks, DateTimeKind.Unspecified);
            _offset = offset.Ticks;
        }

        public DateTimeOffset(DateTime dateTime)
        {
            long off = dateTime.Kind == DateTimeKind.Utc ? 0 : Horloge.OffsetOfLocal(dateTime.Ticks);
            _local = new DateTime(dateTime.Ticks, DateTimeKind.Unspecified);
            _offset = off;
        }

        public DateTimeOffset(DateTime dateTime, TimeSpan offset)
        {
            CheckOffset(offset.Ticks);
            if (dateTime.Kind == DateTimeKind.Utc && offset.Ticks != 0)
                throw new ArgumentException("The UTC Offset for Utc DateTime instances must be 0.");
            _local = new DateTime(dateTime.Ticks, DateTimeKind.Unspecified);
            _offset = offset.Ticks;
        }

        public DateTimeOffset(int year, int month, int day, int hour, int minute, int second, TimeSpan offset)
            : this(new DateTime(year, month, day, hour, minute, second).Ticks, offset) { }

        public DateTimeOffset(int year, int month, int day, int hour, int minute, int second, int millisecond, TimeSpan offset)
            : this(new DateTime(year, month, day, hour, minute, second, millisecond).Ticks, offset) { }

        private static void CheckOffset(long ticks)
        {
            if (ticks % TimeSpan.TicksPerMinute != 0)
                throw new ArgumentException("Offset must be specified in whole minutes.");
            if (ticks < -14 * TimeSpan.TicksPerHour || ticks > 14 * TimeSpan.TicksPerHour)
                throw new ArgumentOutOfRangeException("offset");
        }

        public static DateTimeOffset UtcNow { get { return new DateTimeOffset(Horloge.UtcNowTicks(), TimeSpan.Zero); } }

        public static DateTimeOffset Now
        {
            get
            {
                long utc = Horloge.UtcNowTicks();
                long off = Horloge.LocalOffsetTicks(utc);
                return new DateTimeOffset(utc + off, new TimeSpan(off));
            }
        }

        public DateTime DateTime { get { return _local; } }
        public DateTime UtcDateTime { get { return new DateTime(_local.Ticks - _offset, DateTimeKind.Utc); } }
        public DateTime LocalDateTime { get { return UtcDateTime.ToLocalTime(); } }
        public TimeSpan Offset { get { return new TimeSpan(_offset); } }
        public long Ticks { get { return _local.Ticks; } }
        public long UtcTicks { get { return _local.Ticks - _offset; } }
        public int Year { get { return _local.Year; } }
        public int Month { get { return _local.Month; } }
        public int Day { get { return _local.Day; } }
        public int Hour { get { return _local.Hour; } }
        public int Minute { get { return _local.Minute; } }
        public int Second { get { return _local.Second; } }
        public int Millisecond { get { return _local.Millisecond; } }
        public DayOfWeek DayOfWeek { get { return _local.DayOfWeek; } }
        public DateTime Date { get { return _local.Date; } }
        public TimeSpan TimeOfDay { get { return _local.TimeOfDay; } }

        public long ToUnixTimeSeconds()
        {
            long t = UtcTicks - DateTime.UnixEpochTicks;
            long s = t / TimeSpan.TicksPerSecond;
            return t < 0 && t % TimeSpan.TicksPerSecond != 0 ? s - 1 : s;
        }

        public long ToUnixTimeMilliseconds()
        {
            long t = UtcTicks - DateTime.UnixEpochTicks;
            long ms = t / TimeSpan.TicksPerMillisecond;
            return t < 0 && t % TimeSpan.TicksPerMillisecond != 0 ? ms - 1 : ms;
        }

        public static DateTimeOffset FromUnixTimeSeconds(long seconds)
        {
            if (seconds < -62135596800L || seconds > 253402300799L)
                throw new ArgumentOutOfRangeException("seconds");
            return new DateTimeOffset(DateTime.UnixEpochTicks + seconds * TimeSpan.TicksPerSecond, TimeSpan.Zero);
        }

        public static DateTimeOffset FromUnixTimeMilliseconds(long milliseconds)
        {
            if (milliseconds < -62135596800000L || milliseconds > 253402300799999L)
                throw new ArgumentOutOfRangeException("milliseconds");
            return new DateTimeOffset(DateTime.UnixEpochTicks + milliseconds * TimeSpan.TicksPerMillisecond, TimeSpan.Zero);
        }

        public DateTimeOffset ToUniversalTime() { return new DateTimeOffset(UtcTicks, TimeSpan.Zero); }

        public DateTimeOffset ToLocalTime()
        {
            long off = Horloge.LocalOffsetTicks(UtcTicks);
            return new DateTimeOffset(UtcTicks + off, new TimeSpan(off));
        }

        public DateTimeOffset ToOffset(TimeSpan offset) { return new DateTimeOffset(UtcTicks + offset.Ticks, offset); }

        public DateTimeOffset Add(TimeSpan timeSpan) { return new DateTimeOffset(_local.Ticks + timeSpan.Ticks, Offset); }
        public DateTimeOffset AddSeconds(double seconds) { return new DateTimeOffset(_local.AddSeconds(seconds).Ticks, Offset); }
        public DateTimeOffset AddMilliseconds(double ms) { return new DateTimeOffset(_local.AddMilliseconds(ms).Ticks, Offset); }
        public DateTimeOffset AddMinutes(double minutes) { return new DateTimeOffset(_local.AddMinutes(minutes).Ticks, Offset); }
        public DateTimeOffset AddHours(double hours) { return new DateTimeOffset(_local.AddHours(hours).Ticks, Offset); }
        public DateTimeOffset AddDays(double days) { return new DateTimeOffset(_local.AddDays(days).Ticks, Offset); }
        public TimeSpan Subtract(DateTimeOffset value) { return new TimeSpan(UtcTicks - value.UtcTicks); }

        public static TimeSpan operator -(DateTimeOffset a, DateTimeOffset b) { return new TimeSpan(a.UtcTicks - b.UtcTicks); }
        public static DateTimeOffset operator +(DateTimeOffset a, TimeSpan t) { return a.Add(t); }
        public static DateTimeOffset operator -(DateTimeOffset a, TimeSpan t) { return new DateTimeOffset(a._local.Ticks - t.Ticks, a.Offset); }
        public static bool operator ==(DateTimeOffset a, DateTimeOffset b) { return a.UtcTicks == b.UtcTicks; }
        public static bool operator !=(DateTimeOffset a, DateTimeOffset b) { return a.UtcTicks != b.UtcTicks; }
        public static bool operator <(DateTimeOffset a, DateTimeOffset b) { return a.UtcTicks < b.UtcTicks; }
        public static bool operator >(DateTimeOffset a, DateTimeOffset b) { return a.UtcTicks > b.UtcTicks; }
        public static bool operator <=(DateTimeOffset a, DateTimeOffset b) { return a.UtcTicks <= b.UtcTicks; }
        public static bool operator >=(DateTimeOffset a, DateTimeOffset b) { return a.UtcTicks >= b.UtcTicks; }
        public static implicit operator DateTimeOffset(DateTime dateTime) { return new DateTimeOffset(dateTime); }

        public int CompareTo(DateTimeOffset other)
        {
            return UtcTicks < other.UtcTicks ? -1 : (UtcTicks > other.UtcTicks ? 1 : 0);
        }

        public int CompareTo(object obj)
        {
            if (obj == null)
                return 1;
            if (!(obj is DateTimeOffset))
                throw new ArgumentException("Object must be of type DateTimeOffset.");
            return CompareTo((DateTimeOffset)obj);
        }

        public override bool Equals(object obj)
        {
            return obj is DateTimeOffset && ((DateTimeOffset)obj).UtcTicks == UtcTicks;
        }

        public bool Equals(DateTimeOffset other) { return UtcTicks == other.UtcTicks; }
        public override int GetHashCode() { return UtcDateTime.GetHashCode(); }

        public override string ToString() { return DateTime.Format(_local, "G", _offset); }
        public string ToString(string format) { return DateTime.Format(_local, format, _offset); }
        public string ToString(IFormatProvider provider) { return ToString(); }
        public string ToString(string format, IFormatProvider provider) { return ToString(format); }

        public static DateTimeOffset Parse(string input)
        {
            DateTimeOffset result;
            if (!TryParse(input, out result))
                throw new FormatException(string.Concat("String '", input, "' was not recognized as a valid DateTime."));
            return result;
        }

        public static bool TryParse(string input, out DateTimeOffset result)
        {
            result = MinValue;
            DateTime local;
            long offset;
            bool hasOffset;
            if (!DateTime.TryParseCore(input, out local, out offset, out hasOffset))
                return false;
            if (!hasOffset)
            {
                result = new DateTimeOffset(local);
                return true;
            }
            long utc = local.ToUniversalTime().Ticks;
            result = new DateTimeOffset(utc + offset, new TimeSpan(offset));
            return true;
        }
    }
}
