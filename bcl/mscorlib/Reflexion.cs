// BCL Winlux : System.Type, System.Reflection.Assembly, AssemblyName et
// System.Version (#1869). Licence GPL 3.
// Écrit d'après la documentation publique de l'API (« Type Class »,
// « Assembly Class », « AssemblyName Class », « Version Class ») et
// ECMA-335 ; aucun code de Microsoft, de Mono ou de Wine.
//
// Le moteur crée un seul objet RuntimeType par type et un seul objet
// RuntimeAssembly par assembly (runtime/src/clr/reflection.rs) ; il range
// leur numéro après leurs champs, hors de portée du code managé. Les
// membres `extern` sont servis par le moteur, le reste s'exécute ici.

using System.Reflection;
using System.Runtime.CompilerServices;

namespace System
{
    public abstract partial class Type : MemberInfo
    {
        [MethodImpl(MethodImplOptions.InternalCall)]
        public static extern Type GetTypeFromHandle(RuntimeTypeHandle handle);

        // Type par son nom, éventuellement qualifié d'assembly (#1884) :
        // cherché par le moteur dans l'assembly qui appelle puis mscorlib.
        [MethodImpl(MethodImplOptions.InternalCall)]
        public static extern Type GetType(string typeName);

        [MethodImpl(MethodImplOptions.InternalCall)]
        public static extern Type GetType(string typeName, bool throwOnError);

        [MethodImpl(MethodImplOptions.InternalCall)]
        public static extern Type GetType(string typeName, bool throwOnError, bool ignoreCase);

        public extern override string Name
        {
            [MethodImpl(MethodImplOptions.InternalCall)]
            get;
        }

        public extern virtual string FullName
        {
            [MethodImpl(MethodImplOptions.InternalCall)]
            get;
        }

        public extern virtual string Namespace
        {
            [MethodImpl(MethodImplOptions.InternalCall)]
            get;
        }

        public extern virtual string AssemblyQualifiedName
        {
            [MethodImpl(MethodImplOptions.InternalCall)]
            get;
        }

        public extern virtual Assembly Assembly
        {
            [MethodImpl(MethodImplOptions.InternalCall)]
            get;
        }

        public extern virtual Type BaseType
        {
            [MethodImpl(MethodImplOptions.InternalCall)]
            get;
        }

        public extern bool IsValueType
        {
            [MethodImpl(MethodImplOptions.InternalCall)]
            get;
        }

        public extern bool IsClass
        {
            [MethodImpl(MethodImplOptions.InternalCall)]
            get;
        }

        public extern bool IsInterface
        {
            [MethodImpl(MethodImplOptions.InternalCall)]
            get;
        }

        public extern virtual bool IsEnum
        {
            [MethodImpl(MethodImplOptions.InternalCall)]
            get;
        }

        public override string ToString()
        {
            return FullName;
        }

        // Un objet Type par type : l'égalité est celle des références.
        public static bool operator ==(Type left, Type right)
        {
            return (object)left == (object)right;
        }

        public static bool operator !=(Type left, Type right)
        {
            return (object)left != (object)right;
        }

        public override bool Equals(object o)
        {
            return (object)this == o;
        }

        public override int GetHashCode()
        {
            return base.GetHashCode();
        }
    }

    internal sealed class RuntimeType : Type { }

    public sealed class Version
    {
        private readonly int _major;
        private readonly int _minor;
        private readonly int _build;
        private readonly int _revision;

        public Version() : this(0, 0) { }

        public Version(int major, int minor)
        {
            if (major < 0 || minor < 0)
                throw new ArgumentOutOfRangeException("version", "Version's parameters must be greater than or equal to zero.");
            _major = major;
            _minor = minor;
            _build = -1;
            _revision = -1;
        }

        public Version(int major, int minor, int build) : this(major, minor)
        {
            if (build < 0)
                throw new ArgumentOutOfRangeException("version", "Version's parameters must be greater than or equal to zero.");
            _build = build;
        }

        public Version(int major, int minor, int build, int revision) : this(major, minor, build)
        {
            if (revision < 0)
                throw new ArgumentOutOfRangeException("version", "Version's parameters must be greater than or equal to zero.");
            _revision = revision;
        }

        public int Major { get { return _major; } }
        public int Minor { get { return _minor; } }
        public int Build { get { return _build; } }
        public int Revision { get { return _revision; } }

        // Un composant absent (-1) passe avant tous les autres, comme le
        // documente Version.CompareTo.
        public int CompareTo(Version value)
        {
            if ((object)value == null) return 1;
            if (_major != value._major) return _major > value._major ? 1 : -1;
            if (_minor != value._minor) return _minor > value._minor ? 1 : -1;
            if (_build != value._build) return _build > value._build ? 1 : -1;
            if (_revision != value._revision) return _revision > value._revision ? 1 : -1;
            return 0;
        }

        public bool Equals(Version obj)
        {
            return (object)obj != null && CompareTo(obj) == 0;
        }

        public override bool Equals(object obj)
        {
            return Equals(obj as Version);
        }

        public override int GetHashCode()
        {
            return (_major & 0xF) << 28 | (_minor & 0xFF) << 20 | (_build & 0xFF) << 12 | (_revision & 0xFFF);
        }

        public override string ToString()
        {
            string s = string.Concat(_major, ".", _minor);
            if (_build < 0) return s;
            s = string.Concat(s, ".", _build);
            if (_revision < 0) return s;
            return string.Concat(s, ".", _revision);
        }

        public static bool operator ==(Version v1, Version v2)
        {
            if ((object)v1 == null) return (object)v2 == null;
            return v1.Equals(v2);
        }

        public static bool operator !=(Version v1, Version v2) { return !(v1 == v2); }

        public static bool operator <(Version v1, Version v2)
        {
            if ((object)v1 == null) throw new ArgumentNullException("v1");
            return v1.CompareTo(v2) < 0;
        }

        public static bool operator <=(Version v1, Version v2)
        {
            if ((object)v1 == null) throw new ArgumentNullException("v1");
            return v1.CompareTo(v2) <= 0;
        }

        public static bool operator >(Version v1, Version v2) { return v2 < v1; }
        public static bool operator >=(Version v1, Version v2) { return v2 <= v1; }
    }

    public class TypeLoadException : SystemException
    {
        public TypeLoadException() { }
        public TypeLoadException(string message) : base(message) { }
    }

    public class ArgumentOutOfRangeException : ArgumentException
    {
        public ArgumentOutOfRangeException() { }
        public ArgumentOutOfRangeException(string paramName)
            : base(WithParam("Specified argument was out of the range of valid values.", paramName)) { }

        public ArgumentOutOfRangeException(string paramName, string message)
            : base(WithParam(message, paramName)) { }

        private static string WithParam(string message, string paramName)
        {
            if (string.IsNullOrEmpty(paramName))
                return message;
            return string.Concat(message, "\r\nParameter name: ", paramName);
        }
    }
}

namespace System.IO
{
    public class IOException : SystemException
    {
        public IOException() { }
        public IOException(string message) : base(message) { }
    }

    public class FileNotFoundException : IOException
    {
        public FileNotFoundException() { }
        public FileNotFoundException(string message) : base(message) { }
    }
}

namespace System.Reflection
{
    public abstract class MemberInfo : ICustomAttributeProvider
    {
        public abstract string Name { get; }

        // Type qui déclare le membre ; le moteur le fixe pour les membres
        // qu'il rend (#2008).
        public virtual Type DeclaringType { get { return null; } }
        public virtual Type ReflectedType { get { return DeclaringType; } }
        public virtual MemberTypes MemberType { get { return MemberTypes.Custom; } }

        // Attributs personnalisés, construits à chaque appel comme le fait
        // .NET.
        internal virtual object[] AttributsBruts() { return new object[0]; }

        public virtual object[] GetCustomAttributes(bool inherit) { return AttributsBruts(); }

        public virtual object[] GetCustomAttributes(Type attributeType, bool inherit)
        {
            if ((object)attributeType == null) throw new ArgumentNullException("attributeType");
            return Rtti.Filtrer(GetCustomAttributes(inherit), attributeType);
        }

        public virtual bool IsDefined(Type attributeType, bool inherit)
        {
            return GetCustomAttributes(attributeType, inherit).Length != 0;
        }

        public static bool operator ==(MemberInfo left, MemberInfo right)
        {
            if ((object)left == null) return (object)right == null;
            return left.Equals(right);
        }

        public static bool operator !=(MemberInfo left, MemberInfo right) { return !(left == right); }

        public override bool Equals(object obj) { return base.Equals(obj); }
        public override int GetHashCode() { return base.GetHashCode(); }
    }

    public abstract partial class Assembly
    {
        [MethodImpl(MethodImplOptions.InternalCall)]
        public static extern Assembly GetExecutingAssembly();

        [MethodImpl(MethodImplOptions.InternalCall)]
        public static extern Assembly GetCallingAssembly();

        [MethodImpl(MethodImplOptions.InternalCall)]
        public static extern Assembly GetEntryAssembly();

        public static Assembly GetAssembly(Type type)
        {
            if ((object)type == null)
                throw new ArgumentNullException("type");
            return type.Assembly;
        }

        public extern virtual string FullName
        {
            [MethodImpl(MethodImplOptions.InternalCall)]
            get;
        }

        public extern virtual string Location
        {
            [MethodImpl(MethodImplOptions.InternalCall)]
            get;
        }

        public extern virtual string CodeBase
        {
            [MethodImpl(MethodImplOptions.InternalCall)]
            get;
        }

        public extern virtual string ImageRuntimeVersion
        {
            [MethodImpl(MethodImplOptions.InternalCall)]
            get;
        }

        [MethodImpl(MethodImplOptions.InternalCall)]
        private extern string GetSimpleName();

        [MethodImpl(MethodImplOptions.InternalCall)]
        private extern string GetCultureName();

        [MethodImpl(MethodImplOptions.InternalCall)]
        private extern int GetVersionPart(int index);

        public virtual AssemblyName GetName()
        {
            Version v = new Version(GetVersionPart(0), GetVersionPart(1), GetVersionPart(2), GetVersionPart(3));
            return new AssemblyName(GetSimpleName(), v, GetCultureName(), FullName, CodeBase);
        }

        public override string ToString()
        {
            return FullName;
        }

        public static bool operator ==(Assembly left, Assembly right)
        {
            return (object)left == (object)right;
        }

        public static bool operator !=(Assembly left, Assembly right)
        {
            return (object)left != (object)right;
        }

        public override bool Equals(object o)
        {
            return (object)this == o;
        }

        public override int GetHashCode()
        {
            return base.GetHashCode();
        }
    }

    internal sealed class RuntimeAssembly : Assembly { }

    public sealed class AssemblyName
    {
        private string _name;
        private Version _version;
        private string _culture;
        private string _fullName;
        private string _codeBase;

        public AssemblyName() { }

        internal AssemblyName(string name, Version version, string culture, string fullName, string codeBase)
        {
            _name = name;
            _version = version;
            _culture = culture;
            _fullName = fullName;
            _codeBase = codeBase;
        }

        public string Name { get { return _name; } set { _name = value; _fullName = null; } }
        public Version Version { get { return _version; } set { _version = value; _fullName = null; } }
        public string CultureName { get { return _culture; } set { _culture = value; _fullName = null; } }
        public string CodeBase { get { return _codeBase; } set { _codeBase = value; } }

        public string FullName
        {
            get
            {
                if ((object)_fullName != null)
                    return _fullName;
                if ((object)_name == null)
                    return "";
                string s = _name;
                if ((object)_version != null)
                    s = string.Concat(s, ", Version=", _version.ToString());
                if ((object)_culture != null)
                    s = string.Concat(s, ", Culture=", _culture.Length == 0 ? "neutral" : _culture);
                return s;
            }
        }

        public override string ToString()
        {
            return FullName;
        }
    }
}
