// BCL Winlux : mscorlib écrite en C# (étape 7 du moteur .NET Framework).
// Licence GPL 3. Écrite d'après ECMA-335 et la documentation publique de
// l'API ; aucun code de Microsoft, de Mono ou de Wine.
//
// Les types de ce fichier sont ceux que le moteur connaît déjà (Object,
// String, primitives…) : leur disposition mémoire est fixée par le runtime.
// Une méthode `extern` marquée InternalCall est servie par runtime/src/clr ;
// une méthode avec un corps s'exécute dans l'interpréteur.

using System.Runtime.CompilerServices;

[assembly: System.Reflection.AssemblyVersion("4.0.0.0")]

namespace System
{
    public class Object
    {
        public Object() { }

        [MethodImpl(MethodImplOptions.InternalCall)]
        public extern virtual string ToString();

        [MethodImpl(MethodImplOptions.InternalCall)]
        public extern virtual bool Equals(object obj);

        [MethodImpl(MethodImplOptions.InternalCall)]
        public extern virtual int GetHashCode();

        [MethodImpl(MethodImplOptions.InternalCall)]
        public extern Type GetType();

        public static bool ReferenceEquals(object a, object b)
        {
            return a == b;
        }

        ~Object() { }
    }

    public abstract class ValueType { }
    public abstract class Enum : ValueType { }
    public struct Void { }

    public struct Boolean { private bool m_value; }
    public struct SByte { private sbyte m_value; }
    public struct Byte { private byte m_value; }
    public struct Int16 { private short m_value; }
    public struct UInt16 { private ushort m_value; }
    public struct Int32
    {
        public const int MaxValue = 0x7FFFFFFF;
        public const int MinValue = -MaxValue - 1;
        private int m_value;
    }
    public struct UInt32 { private uint m_value; }
    public struct Int64
    {
        public const long MaxValue = 0x7FFFFFFFFFFFFFFF;
        public const long MinValue = -MaxValue - 1;
        private long m_value;
    }
    public struct UInt64 { private ulong m_value; }
    public struct Single { private float m_value; }
    public struct Double
    {
        private double m_value;

        // NaN est le seul double différent de lui-même (IEEE 754).
#pragma warning disable 1718
        public static bool IsNaN(double d) { return d != d; }
#pragma warning restore 1718
    }
    public unsafe struct IntPtr { private void* m_value; }
    public unsafe struct UIntPtr { private void* m_value; }

    public struct Char
    {
        private char m_value;

        // Espaces Unicode (catégories Zs, Zl, Zp et caractères de contrôle
        // d'espacement), comme la documentation de Char.IsWhiteSpace.
        public static bool IsWhiteSpace(char c)
        {
            int n = c;
            if (n == 0x20 || (n >= 0x09 && n <= 0x0D) || n == 0xA0 || n == 0x85)
                return true;
            if (n < 0x1680)
                return false;
            return n == 0x1680 || (n >= 0x2000 && n <= 0x200A) || n == 0x2028
                || n == 0x2029 || n == 0x202F || n == 0x205F || n == 0x3000;
        }

        public static bool IsDigit(char c)
        {
            return c >= '0' && c <= '9';
        }
    }

    public sealed class String
    {
        public static readonly string Empty = "";

        public extern int Length
        {
            [MethodImpl(MethodImplOptions.InternalCall)]
            get;
        }

        [IndexerName("Chars")]
        public extern char this[int index]
        {
            [MethodImpl(MethodImplOptions.InternalCall)]
            get;
        }

        public static bool IsNullOrEmpty(string value)
        {
            return value == null || value.Length == 0;
        }

        public static bool IsNullOrWhiteSpace(string value)
        {
            if (value == null)
                return true;
            for (int i = 0; i < value.Length; i++)
            {
                if (!char.IsWhiteSpace(value[i]))
                    return false;
            }
            return true;
        }
    }

    public abstract class Array
    {
        public extern int Length
        {
            [MethodImpl(MethodImplOptions.InternalCall)]
            get;
        }
    }

    public abstract class Type
    {
        [MethodImpl(MethodImplOptions.InternalCall)]
        public static extern Type GetTypeFromHandle(RuntimeTypeHandle handle);
    }

    public struct RuntimeTypeHandle { private IntPtr m_value; }
    public struct RuntimeFieldHandle { private IntPtr m_value; }
    public struct RuntimeMethodHandle { private IntPtr m_value; }

    public abstract class Delegate { }
    public abstract class MulticastDelegate : Delegate { }

    public class Exception
    {
        // Message et exception interne sont rangés par le moteur
        // (exceptions.rs) : les constructeurs restent des appels internes.
        [MethodImpl(MethodImplOptions.InternalCall)]
        public extern Exception();

        [MethodImpl(MethodImplOptions.InternalCall)]
        public extern Exception(string message);

        [MethodImpl(MethodImplOptions.InternalCall)]
        public extern Exception(string message, Exception innerException);

        public extern virtual string Message
        {
            [MethodImpl(MethodImplOptions.InternalCall)]
            get;
        }

        public extern Exception InnerException
        {
            [MethodImpl(MethodImplOptions.InternalCall)]
            get;
        }
    }

    public static class Console
    {
        // Sortie console servie par le moteur (bcl.rs) en attendant les flux.
        [MethodImpl(MethodImplOptions.InternalCall)]
        public static extern void WriteLine(string value);

        [MethodImpl(MethodImplOptions.InternalCall)]
        public static extern void Write(string value);
    }

    public interface IDisposable
    {
        void Dispose();
    }

    [AttributeUsage(AttributeTargets.All, Inherited = true, AllowMultiple = false)]
    public abstract class Attribute { }

    [Flags]
    public enum AttributeTargets
    {
        Assembly = 1, Module = 2, Class = 4, Struct = 8, Enum = 16,
        Constructor = 32, Method = 64, Property = 128, Field = 256, Event = 512,
        Interface = 1024, Parameter = 2048, Delegate = 4096, ReturnValue = 8192,
        GenericParameter = 16384, All = 32767,
    }

    [AttributeUsage(AttributeTargets.Class, Inherited = true)]
    public sealed class AttributeUsageAttribute : Attribute
    {
        public AttributeUsageAttribute(AttributeTargets validOn) { }
        public bool AllowMultiple { get { return false; } set { } }
        public bool Inherited { get { return true; } set { } }
    }

    [AttributeUsage(AttributeTargets.Enum)]
    public class FlagsAttribute : Attribute { }

    [AttributeUsage(AttributeTargets.Parameter)]
    public sealed class ParamArrayAttribute : Attribute { }
}

namespace System.Collections
{
    public interface IEnumerator
    {
        bool MoveNext();
        object Current { get; }
        void Reset();
    }

    public interface IEnumerable
    {
        IEnumerator GetEnumerator();
    }
}

namespace System.Reflection
{
    [AttributeUsage(AttributeTargets.Assembly)]
    public sealed class AssemblyVersionAttribute : Attribute
    {
        public AssemblyVersionAttribute(string version) { }
    }

    [AttributeUsage(AttributeTargets.Property)]
    public sealed class DefaultMemberAttribute : Attribute
    {
        public DefaultMemberAttribute(string memberName) { }
    }
}

namespace System.Runtime.CompilerServices
{
    public enum MethodImplOptions
    {
        NoInlining = 8,
        InternalCall = 4096,
    }

    [AttributeUsage(AttributeTargets.Method | AttributeTargets.Constructor)]
    public sealed class MethodImplAttribute : Attribute
    {
        public MethodImplAttribute(MethodImplOptions options) { }
    }

    [AttributeUsage(AttributeTargets.Property)]
    public sealed class IndexerNameAttribute : Attribute
    {
        public IndexerNameAttribute(string indexerName) { }
    }
}
