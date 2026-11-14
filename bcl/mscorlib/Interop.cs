// System.Runtime.InteropServices.Marshal et COMException de la mscorlib
// Winlux (#2026, #2105). Licence GPL 3.
// Écrit d'après la documentation publique de .NET Framework 4.x (MS Learn :
// « Marshal Class » et ses membres, « COMException », « HRESULT_FROM_WIN32 »
// dans winerror.h de mingw-w64) ; aucun code de Microsoft, de Mono ou de
// Wine.
//
// Le moteur (clr/marshal.rs) sert ce qui touche aux métadonnées ou à la
// mémoire native : SizeOf, copie des structures (StructureToPtr,
// PtrToStructure, DestroyStructure, OffsetOf), allocation (LocalAlloc,
// CoTaskMemAlloc) et chaînes natives (page de code ANSI, BSTR). Les
// lectures, écritures et copies brutes sont écrites ici avec des pointeurs.
using System.Runtime.CompilerServices;

namespace System.Runtime.InteropServices
{
    public class COMException : ExternalException
    {
        public COMException() : base("Error HRESULT E_FAIL has been returned from a call to a COM component.", unchecked((int)0x80004005)) { }
        public COMException(string message) : base(message, unchecked((int)0x80004005)) { }
        public COMException(string message, int errorCode) : base(message, errorCode) { }
    }

    public static class Marshal
    {
        public static readonly int SystemDefaultCharSize = 2;
        public static readonly int SystemMaxDBCSCharSize = 1;

        // Formats de chaîne passés au moteur.
        private const int Ansi = 0;
        private const int Unicode = 1;
        private const int Bstr = 2;

        [MethodImpl(MethodImplOptions.InternalCall)] public static extern int SizeOf(Type t);
        [MethodImpl(MethodImplOptions.InternalCall)] public static extern int SizeOf(object structure);
        [MethodImpl(MethodImplOptions.InternalCall)] public static extern int SizeOf<T>();
        [MethodImpl(MethodImplOptions.InternalCall)] public static extern int SizeOf<T>(T structure);
        [MethodImpl(MethodImplOptions.InternalCall)] public static extern int GetLastWin32Error();

        [MethodImpl(MethodImplOptions.InternalCall)] private static extern IntPtr Allouer(long cb, bool coTask);
        [MethodImpl(MethodImplOptions.InternalCall)] private static extern void Liberer(IntPtr p, bool coTask);
        [MethodImpl(MethodImplOptions.InternalCall)] private static extern IntPtr Reallouer(IntPtr p, long cb, bool coTask);
        [MethodImpl(MethodImplOptions.InternalCall)] private static extern void VersNatif(object structure, IntPtr ptr, bool fDeleteOld);
        [MethodImpl(MethodImplOptions.InternalCall)] private static extern object DepuisNatif(IntPtr ptr, Type structureType);
        [MethodImpl(MethodImplOptions.InternalCall)] private static extern void RemplirDepuisNatif(IntPtr ptr, object structure);
        [MethodImpl(MethodImplOptions.InternalCall)] private static extern void Detruire(IntPtr ptr, Type structureType);
        [MethodImpl(MethodImplOptions.InternalCall)] private static extern int Decalage(Type t, string fieldName);
        [MethodImpl(MethodImplOptions.InternalCall)] private static extern string LireChaine(IntPtr ptr, int len, int format);
        [MethodImpl(MethodImplOptions.InternalCall)] private static extern IntPtr EcrireChaine(string s, int format, bool coTask);
        [MethodImpl(MethodImplOptions.InternalCall)] private static extern void LibererBstr(IntPtr ptr);
        [MethodImpl(MethodImplOptions.InternalCall)] private static extern Exception ExceptionSysteme(string name, string message);

        // ── Mémoire native ──

        public static IntPtr AllocHGlobal(int cb) { return Allouer(cb, false); }
        public static IntPtr AllocHGlobal(IntPtr cb) { return Allouer((long)cb, false); }
        public static void FreeHGlobal(IntPtr hglobal) { Liberer(hglobal, false); }
        public static IntPtr ReAllocHGlobal(IntPtr pv, IntPtr cb) { return Reallouer(pv, (long)cb, false); }
        public static IntPtr AllocCoTaskMem(int cb) { return Allouer(cb, true); }
        public static void FreeCoTaskMem(IntPtr ptr) { Liberer(ptr, true); }
        public static IntPtr ReAllocCoTaskMem(IntPtr pv, int cb) { return Reallouer(pv, cb, true); }

        // ── Structures ──

        public static void StructureToPtr(object structure, IntPtr ptr, bool fDeleteOld)
        {
            if (structure == null) throw new ArgumentNullException("structure");
            if (ptr == IntPtr.Zero) throw new ArgumentNullException("ptr");
            VersNatif(structure, ptr, fDeleteOld);
        }

        public static void StructureToPtr<T>(T structure, IntPtr ptr, bool fDeleteOld) { StructureToPtr((object)structure, ptr, fDeleteOld); }

        public static object PtrToStructure(IntPtr ptr, Type structureType)
        {
            if (ptr == IntPtr.Zero) return null;
            if (structureType == null) throw new ArgumentNullException("structureType");
            if (structureType.IsValueType) return DepuisNatif(ptr, structureType);
            // Une classe passe par son constructeur sans paramètre.
            object o = Activator.CreateInstance(structureType);
            RemplirDepuisNatif(ptr, o);
            return o;
        }

        public static void PtrToStructure(IntPtr ptr, object structure)
        {
            if (structure == null) throw new ArgumentNullException("structure");
            if (structure.GetType().IsValueType) throw new ArgumentException("The structure must not be a value class.", "structure");
            if (ptr == IntPtr.Zero) return;
            RemplirDepuisNatif(ptr, structure);
        }

        public static T PtrToStructure<T>(IntPtr ptr) { return (T)PtrToStructure(ptr, typeof(T)); }
        public static void PtrToStructure<T>(IntPtr ptr, T structure) { PtrToStructure(ptr, (object)structure); }

        public static void DestroyStructure(IntPtr ptr, Type structuretype)
        {
            if (ptr == IntPtr.Zero) throw new ArgumentNullException("ptr");
            if (structuretype == null) throw new ArgumentNullException("structuretype");
            Detruire(ptr, structuretype);
        }

        public static void DestroyStructure<T>(IntPtr ptr) { DestroyStructure(ptr, typeof(T)); }

        public static IntPtr OffsetOf(Type t, string fieldName)
        {
            if (t == null) throw new ArgumentNullException("t");
            if (fieldName == null) throw new ArgumentNullException("fieldName");
            return new IntPtr(Decalage(t, fieldName));
        }

        public static IntPtr OffsetOf<T>(string fieldName) { return OffsetOf(typeof(T), fieldName); }

        // ── Chaînes natives ──

        private static int Longueur(int len)
        {
            if (len < 0) throw new ArgumentException("Positive number required.", "len");
            return len;
        }

        public static string PtrToStringAnsi(IntPtr ptr) { return LireChaine(ptr, -1, Ansi); }
        public static string PtrToStringAnsi(IntPtr ptr, int len) { return LireChaine(ptr, Longueur(len), Ansi); }
        public static string PtrToStringUni(IntPtr ptr) { return LireChaine(ptr, -1, Unicode); }
        public static string PtrToStringUni(IntPtr ptr, int len) { return LireChaine(ptr, Longueur(len), Unicode); }
        // « Auto » vaut Unicode sur Windows NT.
        public static string PtrToStringAuto(IntPtr ptr) { return PtrToStringUni(ptr); }
        public static string PtrToStringAuto(IntPtr ptr, int len) { return PtrToStringUni(ptr, len); }

        public static string PtrToStringBSTR(IntPtr ptr)
        {
            if (ptr == IntPtr.Zero) throw new ArgumentNullException("ptr");
            return LireChaine(ptr, -1, Bstr);
        }

        public static IntPtr StringToHGlobalAnsi(string s) { return EcrireChaine(s, Ansi, false); }
        public static IntPtr StringToHGlobalUni(string s) { return EcrireChaine(s, Unicode, false); }
        public static IntPtr StringToHGlobalAuto(string s) { return StringToHGlobalUni(s); }
        public static IntPtr StringToCoTaskMemAnsi(string s) { return EcrireChaine(s, Ansi, true); }
        public static IntPtr StringToCoTaskMemUni(string s) { return EcrireChaine(s, Unicode, true); }
        public static IntPtr StringToCoTaskMemAuto(string s) { return StringToCoTaskMemUni(s); }
        public static IntPtr StringToBSTR(string s) { return EcrireChaine(s, Bstr, false); }
        public static void FreeBSTR(IntPtr ptr) { LibererBstr(ptr); }

        private static unsafe void Effacer(IntPtr s)
        {
            if (s == IntPtr.Zero) return;
            char* c = (char*)s.ToPointer();
            while (*c != '\0') { *c = '\0'; c++; }
        }

        public static void ZeroFreeGlobalAllocUnicode(IntPtr s) { Effacer(s); FreeHGlobal(s); }
        public static void ZeroFreeCoTaskMemUnicode(IntPtr s) { Effacer(s); FreeCoTaskMem(s); }

        // ── Lectures et écritures brutes ──

        public static unsafe byte ReadByte(IntPtr ptr, int ofs) { return *((byte*)ptr.ToPointer() + ofs); }
        public static byte ReadByte(IntPtr ptr) { return ReadByte(ptr, 0); }
        public static unsafe short ReadInt16(IntPtr ptr, int ofs) { return *(short*)((byte*)ptr.ToPointer() + ofs); }
        public static short ReadInt16(IntPtr ptr) { return ReadInt16(ptr, 0); }
        public static unsafe int ReadInt32(IntPtr ptr, int ofs) { return *(int*)((byte*)ptr.ToPointer() + ofs); }
        public static int ReadInt32(IntPtr ptr) { return ReadInt32(ptr, 0); }
        public static unsafe long ReadInt64(IntPtr ptr, int ofs) { return *(long*)((byte*)ptr.ToPointer() + ofs); }
        public static long ReadInt64(IntPtr ptr) { return ReadInt64(ptr, 0); }
        public static IntPtr ReadIntPtr(IntPtr ptr, int ofs) { return new IntPtr(ReadInt64(ptr, ofs)); }
        public static IntPtr ReadIntPtr(IntPtr ptr) { return ReadIntPtr(ptr, 0); }

        public static unsafe void WriteByte(IntPtr ptr, int ofs, byte val) { *((byte*)ptr.ToPointer() + ofs) = val; }
        public static void WriteByte(IntPtr ptr, byte val) { WriteByte(ptr, 0, val); }
        public static unsafe void WriteInt16(IntPtr ptr, int ofs, short val) { *(short*)((byte*)ptr.ToPointer() + ofs) = val; }
        public static void WriteInt16(IntPtr ptr, short val) { WriteInt16(ptr, 0, val); }
        public static void WriteInt16(IntPtr ptr, int ofs, char val) { WriteInt16(ptr, ofs, (short)val); }
        public static void WriteInt16(IntPtr ptr, char val) { WriteInt16(ptr, 0, (short)val); }
        public static unsafe void WriteInt32(IntPtr ptr, int ofs, int val) { *(int*)((byte*)ptr.ToPointer() + ofs) = val; }
        public static void WriteInt32(IntPtr ptr, int val) { WriteInt32(ptr, 0, val); }
        public static unsafe void WriteInt64(IntPtr ptr, int ofs, long val) { *(long*)((byte*)ptr.ToPointer() + ofs) = val; }
        public static void WriteInt64(IntPtr ptr, long val) { WriteInt64(ptr, 0, val); }
        public static void WriteIntPtr(IntPtr ptr, int ofs, IntPtr val) { WriteInt64(ptr, ofs, (long)val); }
        public static void WriteIntPtr(IntPtr ptr, IntPtr val) { WriteIntPtr(ptr, 0, val); }

        // ── Copies entre tableaux gérés et mémoire native ──

        private static void Bornes(Array tableau, int startIndex, int length)
        {
            if (startIndex < 0) throw new ArgumentOutOfRangeException("startIndex", "Index was out of range. Must be non-negative and less than the size of the collection.");
            if (length < 0) throw new ArgumentOutOfRangeException("length", "Non-negative number required.");
            if (startIndex > tableau.Length - length) throw new ArgumentOutOfRangeException("length", "Requested range extends past the end of the array.");
        }

        private static void VersPointeur(Array source, int startIndex, IntPtr destination, int length)
        {
            if (source == null) throw new ArgumentNullException("source");
            if (destination == IntPtr.Zero) throw new ArgumentNullException("destination");
            Bornes(source, startIndex, length);
        }

        private static void DepuisPointeur(IntPtr source, Array destination, int startIndex, int length)
        {
            if (source == IntPtr.Zero) throw new ArgumentNullException("source");
            if (destination == null) throw new ArgumentNullException("destination");
            Bornes(destination, startIndex, length);
        }

        public static unsafe void Copy(byte[] source, int startIndex, IntPtr destination, int length)
        {
            VersPointeur(source, startIndex, destination, length);
            byte* d = (byte*)destination.ToPointer();
            for (int i = 0; i < length; i++) d[i] = source[startIndex + i];
        }

        public static unsafe void Copy(IntPtr source, byte[] destination, int startIndex, int length)
        {
            DepuisPointeur(source, destination, startIndex, length);
            byte* s = (byte*)source.ToPointer();
            for (int i = 0; i < length; i++) destination[startIndex + i] = s[i];
        }

        public static unsafe void Copy(char[] source, int startIndex, IntPtr destination, int length)
        {
            VersPointeur(source, startIndex, destination, length);
            char* d = (char*)destination.ToPointer();
            for (int i = 0; i < length; i++) d[i] = source[startIndex + i];
        }

        public static unsafe void Copy(IntPtr source, char[] destination, int startIndex, int length)
        {
            DepuisPointeur(source, destination, startIndex, length);
            char* s = (char*)source.ToPointer();
            for (int i = 0; i < length; i++) destination[startIndex + i] = s[i];
        }

        public static unsafe void Copy(short[] source, int startIndex, IntPtr destination, int length)
        {
            VersPointeur(source, startIndex, destination, length);
            short* d = (short*)destination.ToPointer();
            for (int i = 0; i < length; i++) d[i] = source[startIndex + i];
        }

        public static unsafe void Copy(IntPtr source, short[] destination, int startIndex, int length)
        {
            DepuisPointeur(source, destination, startIndex, length);
            short* s = (short*)source.ToPointer();
            for (int i = 0; i < length; i++) destination[startIndex + i] = s[i];
        }

        public static unsafe void Copy(int[] source, int startIndex, IntPtr destination, int length)
        {
            VersPointeur(source, startIndex, destination, length);
            int* d = (int*)destination.ToPointer();
            for (int i = 0; i < length; i++) d[i] = source[startIndex + i];
        }

        public static unsafe void Copy(IntPtr source, int[] destination, int startIndex, int length)
        {
            DepuisPointeur(source, destination, startIndex, length);
            int* s = (int*)source.ToPointer();
            for (int i = 0; i < length; i++) destination[startIndex + i] = s[i];
        }

        public static unsafe void Copy(long[] source, int startIndex, IntPtr destination, int length)
        {
            VersPointeur(source, startIndex, destination, length);
            long* d = (long*)destination.ToPointer();
            for (int i = 0; i < length; i++) d[i] = source[startIndex + i];
        }

        public static unsafe void Copy(IntPtr source, long[] destination, int startIndex, int length)
        {
            DepuisPointeur(source, destination, startIndex, length);
            long* s = (long*)source.ToPointer();
            for (int i = 0; i < length; i++) destination[startIndex + i] = s[i];
        }

        public static unsafe void Copy(float[] source, int startIndex, IntPtr destination, int length)
        {
            VersPointeur(source, startIndex, destination, length);
            float* d = (float*)destination.ToPointer();
            for (int i = 0; i < length; i++) d[i] = source[startIndex + i];
        }

        public static unsafe void Copy(IntPtr source, float[] destination, int startIndex, int length)
        {
            DepuisPointeur(source, destination, startIndex, length);
            float* s = (float*)source.ToPointer();
            for (int i = 0; i < length; i++) destination[startIndex + i] = s[i];
        }

        public static unsafe void Copy(double[] source, int startIndex, IntPtr destination, int length)
        {
            VersPointeur(source, startIndex, destination, length);
            double* d = (double*)destination.ToPointer();
            for (int i = 0; i < length; i++) d[i] = source[startIndex + i];
        }

        public static unsafe void Copy(IntPtr source, double[] destination, int startIndex, int length)
        {
            DepuisPointeur(source, destination, startIndex, length);
            double* s = (double*)source.ToPointer();
            for (int i = 0; i < length; i++) destination[startIndex + i] = s[i];
        }

        public static unsafe void Copy(IntPtr[] source, int startIndex, IntPtr destination, int length)
        {
            VersPointeur(source, startIndex, destination, length);
            long* d = (long*)destination.ToPointer();
            for (int i = 0; i < length; i++) d[i] = (long)source[startIndex + i];
        }

        public static unsafe void Copy(IntPtr source, IntPtr[] destination, int startIndex, int length)
        {
            DepuisPointeur(source, destination, startIndex, length);
            long* s = (long*)source.ToPointer();
            for (int i = 0; i < length; i++) destination[startIndex + i] = new IntPtr(s[i]);
        }
        // ── HRESULT ──

        // HRESULT_FROM_WIN32 (winerror.h).
        public static int GetHRForLastWin32Error()
        {
            int e = GetLastWin32Error();
            if ((e & 0x80000000) == 0x80000000) return e;
            return (e & 0x0000FFFF) | unchecked((int)0x80070000);
        }

        private static string Texte(int errorCode)
        {
            return "Exception from HRESULT: 0x" + errorCode.ToString("X8");
        }

        // Les HRESULT que la documentation associe à une exception .NET ;
        // les autres donnent une COMException qui garde le code.
        public static Exception GetExceptionForHR(int errorCode)
        {
            if (errorCode >= 0) return null;
            string m = Texte(errorCode);
            switch ((uint)errorCode)
            {
                case 0x80004001: return ExceptionSysteme("NotImplementedException", m);
                case 0x80004002: return new InvalidCastException(m);
                case 0x80004003: return ExceptionSysteme("NullReferenceException", m);
                case 0x8007000E: return ExceptionSysteme("OutOfMemoryException", m);
                case 0x80070057: return new ArgumentException(m);
                case 0x80070002: return new System.IO.FileNotFoundException(m);
                case 0x80070003: return new System.IO.DirectoryNotFoundException(m);
                case 0x80070005: return new UnauthorizedAccessException(m);
                case 0x80131509: return new InvalidOperationException(m);
                case 0x80131515: return new NotSupportedException(m);
            }
            return new COMException(m, errorCode);
        }

        public static Exception GetExceptionForHR(int errorCode, IntPtr errorInfo) { return GetExceptionForHR(errorCode); }

        public static void ThrowExceptionForHR(int errorCode)
        {
            if (errorCode < 0) throw GetExceptionForHR(errorCode);
        }

        public static void ThrowExceptionForHR(int errorCode, IntPtr errorInfo) { ThrowExceptionForHR(errorCode); }

        // COR_E_EXCEPTION pour une exception sans code COM.
        public static int GetHRForException(Exception e)
        {
            if (e == null) return 0;
            ExternalException x = e as ExternalException;
            if (x != null) return x.ErrorCode;
            return unchecked((int)0x80131500);
        }

        public static bool IsComObject(object o)
        {
            if (o == null) throw new ArgumentNullException("o");
            return false;
        }
    }
}
