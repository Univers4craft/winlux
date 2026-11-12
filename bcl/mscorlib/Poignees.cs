// SafeHandle et ses dérivés de la mscorlib Winlux (#2019). Licence GPL 3.
// Écrit d'après la documentation publique de .NET Framework 4.x (MS Learn :
// « SafeHandle Class », « SafeFileHandle », « SafeWaitHandle »,
// « SafeRegistryHandle ») ; aucun code de Microsoft, de Mono ou de Wine.
//
// Notre ramasse-miettes ne lance pas de finaliseurs : un SafeHandle oublié
// sans Dispose garde son handle ouvert jusqu'à la fin du processus, comme un
// handle Win32 jamais fermé. Le moteur passe le champ `handle` aux appels
// P/Invoke (interop.rs).
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;

namespace System.Runtime.InteropServices
{
    // Pseudo-attributs d'interopérabilité : le compilateur les change en
    // métadonnées (ImplMap, ClassLayout, FieldLayout, FieldMarshal), mais
    // leurs types doivent exister dans la mscorlib (#2019).
    public enum CharSet { None = 1, Ansi = 2, Unicode = 3, Auto = 4 }

    public enum CallingConvention { Winapi = 1, Cdecl = 2, StdCall = 3, ThisCall = 4, FastCall = 5 }

    public enum LayoutKind { Sequential = 0, Explicit = 2, Auto = 3 }

    public enum UnmanagedType
    {
        Bool = 2, I1 = 3, U1 = 4, I2 = 5, U2 = 6, I4 = 7, U4 = 8, I8 = 9, U8 = 10, R4 = 11, R8 = 12,
        Currency = 15, BStr = 19, LPStr = 20, LPWStr = 21, LPTStr = 22, ByValTStr = 23, IUnknown = 25,
        IDispatch = 26, Struct = 27, Interface = 28, SafeArray = 29, ByValArray = 30, SysInt = 31,
        SysUInt = 32, VBByRefStr = 34, AnsiBStr = 35, TBStr = 36, VariantBool = 37, FunctionPtr = 38,
        AsAny = 40, LPArray = 42, LPStruct = 43, CustomMarshaler = 44, Error = 45, IInspectable = 46,
        HString = 47, LPUTF8Str = 48,
    }

    [AttributeUsage(AttributeTargets.Method, Inherited = false)]
    public sealed class DllImportAttribute : Attribute
    {
        private readonly string _dll;
        public string EntryPoint;
        public CharSet CharSet;
        public bool SetLastError;
        public bool ExactSpelling;
        public bool PreserveSig;
        public CallingConvention CallingConvention;
        public bool BestFitMapping;
        public bool ThrowOnUnmappableChar;

        public DllImportAttribute(string dllName) { _dll = dllName; }
        public string Value { get { return _dll; } }
    }

    [AttributeUsage(AttributeTargets.Class | AttributeTargets.Struct, Inherited = false)]
    public sealed class StructLayoutAttribute : Attribute
    {
        private readonly LayoutKind _disposition;
        public int Pack;
        public int Size;
        public CharSet CharSet;

        public StructLayoutAttribute(LayoutKind layoutKind) { _disposition = layoutKind; }
        public StructLayoutAttribute(short layoutKind) { _disposition = (LayoutKind)layoutKind; }
        public LayoutKind Value { get { return _disposition; } }
    }

    [AttributeUsage(AttributeTargets.Field, Inherited = false)]
    public sealed class FieldOffsetAttribute : Attribute
    {
        private readonly int _decalage;
        public FieldOffsetAttribute(int offset) { _decalage = offset; }
        public int Value { get { return _decalage; } }
    }

    [AttributeUsage(AttributeTargets.Parameter | AttributeTargets.Field | AttributeTargets.ReturnValue, Inherited = false)]
    public sealed class MarshalAsAttribute : Attribute
    {
        private readonly UnmanagedType _type;
        public UnmanagedType ArraySubType;
        public int SizeConst;
        public short SizeParamIndex;
        public string MarshalType;
        public Type MarshalTypeRef;
        public string MarshalCookie;
        public UnmanagedType SafeArraySubType;
        public Type SafeArrayUserDefinedSubType;
        public int IidParameterIndex;

        public MarshalAsAttribute(UnmanagedType unmanagedType) { _type = unmanagedType; }
        public MarshalAsAttribute(short unmanagedType) { _type = (UnmanagedType)unmanagedType; }
        public UnmanagedType Value { get { return _type; } }
    }

    [AttributeUsage(AttributeTargets.Parameter, Inherited = false)]
    public sealed class InAttribute : Attribute { }

    [AttributeUsage(AttributeTargets.Parameter, Inherited = false)]
    public sealed class OutAttribute : Attribute { }

    [AttributeUsage(AttributeTargets.Parameter, Inherited = false)]
    public sealed class OptionalAttribute : Attribute { }

    [AttributeUsage(AttributeTargets.Method, Inherited = false)]
    public sealed class PreserveSigAttribute : Attribute { }

    public abstract class SafeHandle : IDisposable
    {
        // Premier champ : le moteur le lit pour un appel P/Invoke.
        protected IntPtr handle;
        private readonly bool _proprietaire;
        private bool _ferme;
        private int _references;
        private bool _fermerAuDernier;

        protected SafeHandle(IntPtr invalidHandleValue, bool ownsHandle)
        {
            handle = invalidHandleValue;
            _proprietaire = ownsHandle;
        }

        public bool IsClosed { get { return _ferme; } }
        public abstract bool IsInvalid { get; }

        protected abstract bool ReleaseHandle();

        protected void SetHandle(IntPtr handle) { this.handle = handle; }
        public IntPtr DangerousGetHandle() { return handle; }
        public void SetHandleAsInvalid() { _ferme = true; }

        public void DangerousAddRef(ref bool success)
        {
            lock (this)
            {
                if (_ferme) throw new ObjectDisposedException(null, "Safe handle has been closed");
                _references++;
                success = true;
            }
        }

        public void DangerousRelease()
        {
            bool liberer;
            lock (this)
            {
                if (_references > 0) _references--;
                liberer = _fermerAuDernier && _references == 0;
            }
            if (liberer) Liberer();
        }

        public void Close() { Dispose(true); }
        public void Dispose() { Dispose(true); }

        protected virtual void Dispose(bool disposing)
        {
            lock (this)
            {
                if (_ferme) return;
                if (_references > 0) { _fermerAuDernier = true; return; }
            }
            Liberer();
        }

        private void Liberer()
        {
            lock (this)
            {
                if (_ferme) return;
                _ferme = true;
            }
            if (_proprietaire && !IsInvalid) ReleaseHandle();
        }
    }

    public abstract class CriticalHandle : IDisposable
    {
        protected IntPtr handle;
        private bool _ferme;

        protected CriticalHandle(IntPtr invalidHandleValue) { handle = invalidHandleValue; }

        public bool IsClosed { get { return _ferme; } }
        public abstract bool IsInvalid { get; }
        protected abstract bool ReleaseHandle();
        protected void SetHandle(IntPtr handle) { this.handle = handle; }
        public void SetHandleAsInvalid() { _ferme = true; }
        public void Close() { Dispose(true); }
        public void Dispose() { Dispose(true); }

        protected virtual void Dispose(bool disposing)
        {
            if (_ferme) return;
            _ferme = true;
            if (!IsInvalid) ReleaseHandle();
        }
    }
}

namespace Microsoft.Win32.SafeHandles
{
    using System;

    public abstract class SafeHandleZeroOrMinusOneIsInvalid : SafeHandle
    {
        protected SafeHandleZeroOrMinusOneIsInvalid(bool ownsHandle) : base(IntPtr.Zero, ownsHandle) { }

        public override bool IsInvalid
        {
            get { long h = handle.ToInt64(); return h == 0 || h == -1; }
        }
    }

    public abstract class SafeHandleMinusOneIsInvalid : SafeHandle
    {
        protected SafeHandleMinusOneIsInvalid(bool ownsHandle) : base(new IntPtr(-1), ownsHandle) { }

        public override bool IsInvalid { get { return handle.ToInt64() == -1; } }
    }

    internal static class Noyau
    {
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern int Fermer(long h);
    }

    public sealed class SafeFileHandle : SafeHandleZeroOrMinusOneIsInvalid
    {
        private SafeFileHandle() : base(true) { }

        public SafeFileHandle(IntPtr preexistingHandle, bool ownsHandle) : base(ownsHandle) { SetHandle(preexistingHandle); }

        protected override bool ReleaseHandle() { return Noyau.Fermer(handle.ToInt64()) == 0; }
    }

    public sealed class SafeWaitHandle : SafeHandleZeroOrMinusOneIsInvalid
    {
        private SafeWaitHandle() : base(true) { }

        public SafeWaitHandle(IntPtr existingHandle, bool ownsHandle) : base(ownsHandle) { SetHandle(existingHandle); }

        protected override bool ReleaseHandle() { return Noyau.Fermer(handle.ToInt64()) == 0; }
    }

    public sealed class SafeRegistryHandle : SafeHandleZeroOrMinusOneIsInvalid
    {
        internal SafeRegistryHandle() : base(true) { }

        public SafeRegistryHandle(IntPtr preexistingHandle, bool ownsHandle) : base(ownsHandle) { SetHandle(preexistingHandle); }

        protected override bool ReleaseHandle() { return Microsoft.Win32.RegistreNatif.Fermer(handle.ToInt64()) == 0; }
    }
}
