// IntPtr et UIntPtr de la mscorlib Winlux (#2018). Licence GPL 3. Écrit
// d'après la documentation publique de .NET Framework 4.x (MS Learn :
// « IntPtr Struct », « UIntPtr Struct ») ; aucun code de Microsoft, de Mono
// ou de Wine.
//
// Le moteur range un IntPtr comme un entier natif de 8 octets. Les membres
// passent par « nint » et « nuint », que C# fait correspondre à ces deux
// types : les conversions deviennent de simples conv.i / conv.i8 en IL,
// sans toucher au champ m_value.
namespace System
{
    public unsafe partial struct IntPtr : IEquatable<IntPtr>
    {
        public static readonly IntPtr Zero;

        public IntPtr(int value) { this = (nint)value; }
        public IntPtr(long value) { this = (nint)value; }
        public IntPtr(void* value) { this = (nint)value; }

        public static int Size { get { return 8; } }

        public int ToInt32() { return checked((int)(nint)this); }
        public long ToInt64() { return (nint)this; }
        public void* ToPointer() { return (void*)(nint)this; }

        public static explicit operator IntPtr(int value) { return (nint)value; }
        public static explicit operator IntPtr(long value) { return (nint)value; }
        public static explicit operator IntPtr(void* value) { return (nint)value; }
        public static explicit operator int(IntPtr value) { return checked((int)(nint)value); }
        public static explicit operator long(IntPtr value) { return (nint)value; }
        public static explicit operator void*(IntPtr value) { return (void*)(nint)value; }

        public static bool operator ==(IntPtr value1, IntPtr value2) { return (nint)value1 == (nint)value2; }
        public static bool operator !=(IntPtr value1, IntPtr value2) { return (nint)value1 != (nint)value2; }
        public static IntPtr operator +(IntPtr pointer, int offset) { return (nint)pointer + offset; }
        public static IntPtr operator -(IntPtr pointer, int offset) { return (nint)pointer - offset; }
        public static IntPtr Add(IntPtr pointer, int offset) { return pointer + offset; }
        public static IntPtr Subtract(IntPtr pointer, int offset) { return pointer - offset; }

        public bool Equals(IntPtr other) { return (nint)this == (nint)other; }
        public override bool Equals(object obj) { return obj is IntPtr && (nint)(IntPtr)obj == (nint)this; }
        public override int GetHashCode() { long v = (nint)this; return (int)v ^ (int)(v >> 32); }
        public override string ToString() { return ((long)(nint)this).ToString(); }
        public string ToString(string format) { return ((long)(nint)this).ToString(format); }
    }

    public unsafe partial struct UIntPtr : IEquatable<UIntPtr>
    {
        public static readonly UIntPtr Zero;

        public UIntPtr(uint value) { this = (nuint)value; }
        public UIntPtr(ulong value) { this = (nuint)value; }
        public UIntPtr(void* value) { this = (nuint)value; }

        public static int Size { get { return 8; } }

        public uint ToUInt32() { return checked((uint)(nuint)this); }
        public ulong ToUInt64() { return (nuint)this; }
        public void* ToPointer() { return (void*)(nuint)this; }

        public static explicit operator UIntPtr(uint value) { return (nuint)value; }
        public static explicit operator UIntPtr(ulong value) { return (nuint)value; }
        public static explicit operator UIntPtr(void* value) { return (nuint)value; }
        public static explicit operator uint(UIntPtr value) { return checked((uint)(nuint)value); }
        public static explicit operator ulong(UIntPtr value) { return (nuint)value; }
        public static explicit operator void*(UIntPtr value) { return (void*)(nuint)value; }

        public static bool operator ==(UIntPtr value1, UIntPtr value2) { return (nuint)value1 == (nuint)value2; }
        public static bool operator !=(UIntPtr value1, UIntPtr value2) { return (nuint)value1 != (nuint)value2; }

        public bool Equals(UIntPtr other) { return (nuint)this == (nuint)other; }
        public override bool Equals(object obj) { return obj is UIntPtr && (nuint)(UIntPtr)obj == (nuint)this; }
        public override int GetHashCode() { ulong v = (nuint)this; return (int)v ^ (int)(v >> 32); }
        public override string ToString() { return ((ulong)(nuint)this).ToString(); }
    }
}
