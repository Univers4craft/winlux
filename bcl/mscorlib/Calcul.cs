// System.Math, MidpointRounding, System.GC et GCCollectionMode de la
// mscorlib Winlux (#2077). Licence GPL 3.
// Écrit d'après la documentation publique de .NET Framework 4.x (MS Learn :
// « Math Class », « Math.Round », « Math.Max », « Math.Min », « Math.Abs »,
// « Math.Sign », « Math.IEEERemainder », « Math.Log(Double, Double) »,
// « GC Class ») et IEEE 754 ; aucun code de Microsoft, de Mono ou de Wine.
//
// Avant #2077, Math était servie en entier par le moteur, qui ne voyait
// pas les types déclarés : Round arrondissait loin de zéro au lieu de
// l'arrondi bancaire, Round(x, n) rendait n, Min/Max comparaient les non
// signés comme des signés et oubliaient NaN, Abs(int.MinValue) ne levait
// rien. Ici chaque surcharge est typée ; seules les fonctions
// transcendantes et Floor/Ceiling/Truncate restent « extern », servies par
// bcl.rs (math_dispatch) avec un seul double ou deux.
using System.Runtime.CompilerServices;

namespace System
{
    public enum MidpointRounding
    {
        ToEven = 0,
        AwayFromZero = 1,
    }

    public static class Math
    {
        public const double PI = 3.14159265358979323846;
        public const double E = 2.7182818284590452354;

        // 2^52 : au-delà, un double n'a plus de partie fractionnaire.
        private const double SansFraction = 4503599627370496.0;
        // Round(x, n) rend x tel quel au-delà de 1e16, comme .NET Framework.
        private const double LimiteArrondi = 1e16;
        private const int ChiffresMax = 15;

        // ── Servies par le moteur ──
        [MethodImpl(MethodImplOptions.InternalCall)] public static extern double Floor(double d);
        [MethodImpl(MethodImplOptions.InternalCall)] public static extern double Ceiling(double a);
        [MethodImpl(MethodImplOptions.InternalCall)] public static extern double Truncate(double d);
        [MethodImpl(MethodImplOptions.InternalCall)] public static extern double Sqrt(double d);
        [MethodImpl(MethodImplOptions.InternalCall)] public static extern double Pow(double x, double y);
        [MethodImpl(MethodImplOptions.InternalCall)] public static extern double Exp(double d);
        [MethodImpl(MethodImplOptions.InternalCall)] public static extern double Log(double d);
        [MethodImpl(MethodImplOptions.InternalCall)] public static extern double Log10(double d);
        [MethodImpl(MethodImplOptions.InternalCall)] public static extern double Sin(double a);
        [MethodImpl(MethodImplOptions.InternalCall)] public static extern double Cos(double d);
        [MethodImpl(MethodImplOptions.InternalCall)] public static extern double Tan(double a);
        [MethodImpl(MethodImplOptions.InternalCall)] public static extern double Asin(double d);
        [MethodImpl(MethodImplOptions.InternalCall)] public static extern double Acos(double d);
        [MethodImpl(MethodImplOptions.InternalCall)] public static extern double Atan(double d);
        [MethodImpl(MethodImplOptions.InternalCall)] public static extern double Atan2(double y, double x);
        [MethodImpl(MethodImplOptions.InternalCall)] public static extern double Sinh(double value);
        [MethodImpl(MethodImplOptions.InternalCall)] public static extern double Cosh(double value);
        [MethodImpl(MethodImplOptions.InternalCall)] public static extern double Tanh(double value);

        // Log(a, base) suit les cas particuliers documentés.
        public static double Log(double a, double newBase)
        {
            if (double.IsNaN(a)) return a;
            if (double.IsNaN(newBase)) return newBase;
            if (newBase == 1) return double.NaN;
            if (a != 1 && (newBase == 0 || double.IsPositiveInfinity(newBase))) return double.NaN;
            return Log(a) / Log(newBase);
        }

        // x - y × Round(x / y) ; un reste nul prend le signe de x.
        public static double IEEERemainder(double x, double y)
        {
            if (double.IsNaN(x)) return x;
            if (double.IsNaN(y)) return y;
            if (y == 0 || double.IsInfinity(x)) return double.NaN;
            if (double.IsInfinity(y)) return x;
            double r = x - y * Round(x / y);
            if (r == 0) return x < 0 ? -0.0 : 0.0;
            return r;
        }

        // ── Arrondis ──

        // Arrondi bancaire : un milieu exact va vers le pair.
        public static double Round(double a)
        {
            if (!(a > -SansFraction && a < SansFraction)) return a;
            double f = Floor(a);
            double d = a - f;
            double r;
            if (d > 0.5) r = f + 1;
            else if (d < 0.5) r = f;
            else r = Floor(f / 2) * 2 == f ? f : f + 1;
            if (r == 0) return a < 0 ? -0.0 : 0.0;
            return r;
        }

        public static double Round(double value, int digits) { return Round(value, digits, MidpointRounding.ToEven); }

        public static double Round(double value, MidpointRounding mode) { return Round(value, 0, mode); }

        public static double Round(double value, int digits, MidpointRounding mode)
        {
            if (digits < 0 || digits > ChiffresMax)
                throw new ArgumentOutOfRangeException("digits", "Rounding digits must be between 0 and 15, inclusive.");
            if (mode != MidpointRounding.ToEven && mode != MidpointRounding.AwayFromZero)
                throw new ArgumentException("The value '" + ((int)mode).ToString() + "' is not valid for this usage of the type MidpointRounding.", "mode");
            if (!(value > -LimiteArrondi && value < LimiteArrondi)) return value;
            double echelle = 1;
            for (int i = 0; i < digits; i++) echelle *= 10;
            double v = value * echelle;
            if (mode == MidpointRounding.AwayFromZero)
            {
                double t = Truncate(v);
                double fraction = v - t;
                if (fraction >= 0.5) t += 1;
                else if (fraction <= -0.5) t -= 1;
                v = t;
                if (v == 0) v = value < 0 ? -0.0 : 0.0;
            }
            else
            {
                v = Round(v);
            }
            return v / echelle;
        }

        // ── Valeur absolue ──

        private const string MinimumNegatif = "Negating the minimum value of a twos complement number is invalid.";

        public static sbyte Abs(sbyte value)
        {
            if (value >= 0) return value;
            if (value == sbyte.MinValue) throw new OverflowException(MinimumNegatif);
            return (sbyte)(-value);
        }

        public static short Abs(short value)
        {
            if (value >= 0) return value;
            if (value == short.MinValue) throw new OverflowException(MinimumNegatif);
            return (short)(-value);
        }

        public static int Abs(int value)
        {
            if (value >= 0) return value;
            if (value == int.MinValue) throw new OverflowException(MinimumNegatif);
            return -value;
        }

        public static long Abs(long value)
        {
            if (value >= 0) return value;
            if (value == long.MinValue) throw new OverflowException(MinimumNegatif);
            return -value;
        }

        // 0 - x rend +0 pour -0 et laisse NaN tel quel.
        public static float Abs(float value) { return value <= 0 ? 0 - value : value; }
        public static double Abs(double value) { return value <= 0 ? 0 - value : value; }

        // ── Minimum et maximum : chaque type garde son ordre ──

        public static byte Max(byte val1, byte val2) { return val1 >= val2 ? val1 : val2; }
        public static sbyte Max(sbyte val1, sbyte val2) { return val1 >= val2 ? val1 : val2; }
        public static short Max(short val1, short val2) { return val1 >= val2 ? val1 : val2; }
        public static ushort Max(ushort val1, ushort val2) { return val1 >= val2 ? val1 : val2; }
        public static int Max(int val1, int val2) { return val1 >= val2 ? val1 : val2; }
        public static uint Max(uint val1, uint val2) { return val1 >= val2 ? val1 : val2; }
        public static long Max(long val1, long val2) { return val1 >= val2 ? val1 : val2; }
        public static ulong Max(ulong val1, ulong val2) { return val1 >= val2 ? val1 : val2; }

        // NaN d'un côté ou de l'autre donne NaN.
        public static float Max(float val1, float val2)
        {
            if (val1 > val2) return val1;
            if (float.IsNaN(val1)) return val1;
            return val2;
        }

        public static double Max(double val1, double val2)
        {
            if (val1 > val2) return val1;
            if (double.IsNaN(val1)) return val1;
            return val2;
        }

        public static byte Min(byte val1, byte val2) { return val1 <= val2 ? val1 : val2; }
        public static sbyte Min(sbyte val1, sbyte val2) { return val1 <= val2 ? val1 : val2; }
        public static short Min(short val1, short val2) { return val1 <= val2 ? val1 : val2; }
        public static ushort Min(ushort val1, ushort val2) { return val1 <= val2 ? val1 : val2; }
        public static int Min(int val1, int val2) { return val1 <= val2 ? val1 : val2; }
        public static uint Min(uint val1, uint val2) { return val1 <= val2 ? val1 : val2; }
        public static long Min(long val1, long val2) { return val1 <= val2 ? val1 : val2; }
        public static ulong Min(ulong val1, ulong val2) { return val1 <= val2 ? val1 : val2; }

        public static float Min(float val1, float val2)
        {
            if (val1 < val2) return val1;
            if (float.IsNaN(val1)) return val1;
            return val2;
        }

        public static double Min(double val1, double val2)
        {
            if (val1 < val2) return val1;
            if (double.IsNaN(val1)) return val1;
            return val2;
        }

        // ── Signe ──

        public static int Sign(sbyte value) { return value < 0 ? -1 : value > 0 ? 1 : 0; }
        public static int Sign(short value) { return value < 0 ? -1 : value > 0 ? 1 : 0; }
        public static int Sign(int value) { return value < 0 ? -1 : value > 0 ? 1 : 0; }
        public static int Sign(long value) { return value < 0 ? -1 : value > 0 ? 1 : 0; }

        private const string PasDeNaN = "Function does not accept floating point Not-a-Number values.";

        public static int Sign(float value)
        {
            if (value < 0) return -1;
            if (value > 0) return 1;
            if (value == 0) return 0;
            throw new ArithmeticException(PasDeNaN);
        }

        public static int Sign(double value)
        {
            if (value < 0) return -1;
            if (value > 0) return 1;
            if (value == 0) return 0;
            throw new ArithmeticException(PasDeNaN);
        }

        // ── Entiers ──

        public static long BigMul(int a, int b) { return (long)a * b; }

        public static int DivRem(int a, int b, out int result)
        {
            result = a % b;
            return a / b;
        }

        public static long DivRem(long a, long b, out long result)
        {
            result = a % b;
            return a / b;
        }
    }

    public enum GCCollectionMode
    {
        Default = 0,
        Forced = 1,
        Optimized = 2,
    }

    // Winlux n'a qu'une génération : tout objet est en génération 0 et
    // une collecte est toujours complète.
    public static class GC
    {
        // Servie par il.rs : collecte complète.
        [MethodImpl(MethodImplOptions.InternalCall)] public static extern void Collect();
        // Servie par bcl.rs : octets alloués sur le tas managé.
        [MethodImpl(MethodImplOptions.InternalCall)] private static extern long Alloue();

        public static int MaxGeneration { get { return 2; } }

        private static void Generation(int generation)
        {
            if (generation < 0) throw new ArgumentOutOfRangeException("generation", "Non-negative number required.");
        }

        public static void Collect(int generation) { Generation(generation); Collect(); }
        public static void Collect(int generation, GCCollectionMode mode) { Collect(generation, mode, true); }
        public static void Collect(int generation, GCCollectionMode mode, bool blocking) { Collect(generation, mode, blocking, false); }

        public static void Collect(int generation, GCCollectionMode mode, bool blocking, bool compacting)
        {
            Generation(generation);
            if (mode < GCCollectionMode.Default || mode > GCCollectionMode.Optimized)
                throw new ArgumentOutOfRangeException("mode", "Enum value was out of legal range.");
            if (mode != GCCollectionMode.Optimized) Collect();
        }

        public static int CollectionCount(int generation) { Generation(generation); return 0; }
        public static int GetGeneration(object obj) { return 0; }

        public static long GetTotalMemory(bool forceFullCollection)
        {
            if (forceFullCollection) Collect();
            return Alloue();
        }

        public static void SuppressFinalize(object obj)
        {
            if (obj == null) throw new ArgumentNullException("obj");
        }

        public static void ReRegisterForFinalize(object obj)
        {
            if (obj == null) throw new ArgumentNullException("obj");
        }

        public static void KeepAlive(object obj) { }
        public static void WaitForPendingFinalizers() { }

        public static void AddMemoryPressure(long bytesAllocated)
        {
            if (bytesAllocated <= 0) throw new ArgumentOutOfRangeException("bytesAllocated", "Positive number required.");
        }

        public static void RemoveMemoryPressure(long bytesAllocated)
        {
            if (bytesAllocated <= 0) throw new ArgumentOutOfRangeException("bytesAllocated", "Positive number required.");
        }
    }
}
