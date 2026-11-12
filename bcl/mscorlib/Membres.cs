// Réflexion sur les membres de la mscorlib Winlux (#2008) : MethodInfo,
// ConstructorInfo, FieldInfo, PropertyInfo, ParameterInfo, BindingFlags,
// attributs personnalisés, Activator et les méthodes statiques d'Enum.
// Source : la documentation publique de .NET Framework 4.x (MS Learn) pour
// les signatures, les exceptions et les valeurs des énumérations ; ECMA-335
// §II.23.1 pour les drapeaux de méthode, de champ et de type.
// Le moteur (runtime/src/clr/membres.rs) ne sert que les primitives de
// Rtti ; le filtrage, la recherche de surcharge et le reste sont ici.
using System.Collections.Generic;
using System.Globalization;
using System.Reflection;
using System.Runtime.CompilerServices;

namespace System.Reflection
{
    [Flags]
    public enum BindingFlags
    {
        Default = 0,
        IgnoreCase = 1,
        DeclaredOnly = 2,
        Instance = 4,
        Static = 8,
        Public = 16,
        NonPublic = 32,
        FlattenHierarchy = 64,
        InvokeMethod = 256,
        CreateInstance = 512,
        GetField = 1024,
        SetField = 2048,
        GetProperty = 4096,
        SetProperty = 8192,
        PutDispProperty = 16384,
        PutRefDispProperty = 32768,
        ExactBinding = 65536,
        SuppressChangeType = 131072,
        OptionalParamBinding = 262144,
        IgnoreReturn = 16777216,
    }

    [Flags]
    public enum MemberTypes
    {
        Constructor = 1,
        Event = 2,
        Field = 4,
        Method = 8,
        Property = 16,
        TypeInfo = 32,
        Custom = 64,
        NestedType = 128,
        All = 191,
    }

    [Flags]
    public enum MethodAttributes
    {
        MemberAccessMask = 0x0007,
        PrivateScope = 0x0000,
        Private = 0x0001,
        FamANDAssem = 0x0002,
        Assembly = 0x0003,
        Family = 0x0004,
        FamORAssem = 0x0005,
        Public = 0x0006,
        Static = 0x0010,
        Final = 0x0020,
        Virtual = 0x0040,
        HideBySig = 0x0080,
        VtableLayoutMask = 0x0100,
        NewSlot = 0x0100,
        Abstract = 0x0400,
        SpecialName = 0x0800,
        RTSpecialName = 0x1000,
    }

    [Flags]
    public enum FieldAttributes
    {
        FieldAccessMask = 0x0007,
        PrivateScope = 0x0000,
        Private = 0x0001,
        FamANDAssem = 0x0002,
        Assembly = 0x0003,
        Family = 0x0004,
        FamORAssem = 0x0005,
        Public = 0x0006,
        Static = 0x0010,
        InitOnly = 0x0020,
        Literal = 0x0040,
        NotSerialized = 0x0080,
        SpecialName = 0x0200,
        HasDefault = 0x8000,
    }

    [Flags]
    public enum TypeAttributes
    {
        VisibilityMask = 0x07,
        NotPublic = 0x00,
        Public = 0x01,
        NestedPublic = 0x02,
        NestedPrivate = 0x03,
        Interface = 0x20,
        Abstract = 0x80,
        Sealed = 0x100,
        SpecialName = 0x400,
        Serializable = 0x2000,
    }

    [Flags]
    public enum ParameterAttributes
    {
        None = 0,
        In = 1,
        Out = 2,
        Optional = 16,
        HasDefault = 0x1000,
    }

    public interface ICustomAttributeProvider
    {
        object[] GetCustomAttributes(bool inherit);
        object[] GetCustomAttributes(Type attributeType, bool inherit);
        bool IsDefined(Type attributeType, bool inherit);
    }

    public class TargetInvocationException : ApplicationException
    {
        public TargetInvocationException(Exception inner) : base("Exception has been thrown by the target of an invocation.", inner) { }
        public TargetInvocationException(string message, Exception inner) : base(message, inner) { }
    }

    public class TargetException : ApplicationException
    {
        public TargetException() : base("Non-static method requires a target.") { }
        public TargetException(string message) : base(message) { }
    }

    public sealed class TargetParameterCountException : ApplicationException
    {
        public TargetParameterCountException() : base("Parameter count mismatch.") { }
        public TargetParameterCountException(string message) : base(message) { }
    }

    public sealed class AmbiguousMatchException : SystemException
    {
        public AmbiguousMatchException() : base("Ambiguous match found.") { }
        public AmbiguousMatchException(string message) : base(message) { }
    }

    // Primitives servies par le moteur (runtime/src/clr/membres.rs). Une
    // poignée vaut (indice de l'assembly << 32) | ligne de la table.
    internal static class Rtti
    {
        [MethodImpl(MethodImplOptions.InternalCall)] internal static extern long[] Methodes(Type t);
        [MethodImpl(MethodImplOptions.InternalCall)] internal static extern long[] Champs(Type t);
        [MethodImpl(MethodImplOptions.InternalCall)] internal static extern long[] Proprietes(Type t);
        [MethodImpl(MethodImplOptions.InternalCall)] internal static extern int DrapeauxType(Type t);
        [MethodImpl(MethodImplOptions.InternalCall)] internal static extern Type[] ArgumentsGeneriques(Type t);
        [MethodImpl(MethodImplOptions.InternalCall)] internal static extern int AriteGenerique(Type t);
        [MethodImpl(MethodImplOptions.InternalCall)] internal static extern Type DefinitionGenerique(Type t);
        [MethodImpl(MethodImplOptions.InternalCall)] internal static extern Type Instancier(Type definition, Type[] arguments);
        [MethodImpl(MethodImplOptions.InternalCall)] internal static extern bool Assignable(Type cible, Type depuis);
        [MethodImpl(MethodImplOptions.InternalCall)] internal static extern Type[] Interfaces(Type t);
        [MethodImpl(MethodImplOptions.InternalCall)] internal static extern object[] AttributsType(Type t);
        [MethodImpl(MethodImplOptions.InternalCall)] internal static extern object[] AttributsAssembly(long assembly);
        [MethodImpl(MethodImplOptions.InternalCall)] internal static extern long AssemblyDe(Assembly a);
        [MethodImpl(MethodImplOptions.InternalCall)] internal static extern Assembly AssemblyParNom(string nom);
        [MethodImpl(MethodImplOptions.InternalCall)] internal static extern Type TypeDansAssembly(long assembly, string nom, bool ignorerCasse);
        [MethodImpl(MethodImplOptions.InternalCall)] internal static extern Type[] TypesAssembly(long assembly);

        [MethodImpl(MethodImplOptions.InternalCall)] internal static extern string NomMethode(long h);
        [MethodImpl(MethodImplOptions.InternalCall)] internal static extern int DrapeauxMethode(long h);
        [MethodImpl(MethodImplOptions.InternalCall)] internal static extern Type Retour(long h, Type proprietaire);
        [MethodImpl(MethodImplOptions.InternalCall)] internal static extern Type[] TypesParametres(long h, Type proprietaire);
        [MethodImpl(MethodImplOptions.InternalCall)] internal static extern long ParametresParReference(long h);
        [MethodImpl(MethodImplOptions.InternalCall)] internal static extern string[] NomsParametres(long h);
        [MethodImpl(MethodImplOptions.InternalCall)] internal static extern int AriteMethode(long h);
        [MethodImpl(MethodImplOptions.InternalCall)] internal static extern object Invoquer(long h, Type proprietaire, object cible, object[] arguments);
        [MethodImpl(MethodImplOptions.InternalCall)] internal static extern object[] AttributsMethode(long h);

        [MethodImpl(MethodImplOptions.InternalCall)] internal static extern string NomChamp(long h);
        [MethodImpl(MethodImplOptions.InternalCall)] internal static extern int DrapeauxChamp(long h);
        [MethodImpl(MethodImplOptions.InternalCall)] internal static extern Type TypeChamp(long h, Type proprietaire);
        [MethodImpl(MethodImplOptions.InternalCall)] internal static extern object Constante(long h, Type proprietaire);
        [MethodImpl(MethodImplOptions.InternalCall)] internal static extern object LireChamp(long h, Type proprietaire, object cible);
        [MethodImpl(MethodImplOptions.InternalCall)] internal static extern void EcrireChamp(long h, Type proprietaire, object cible, object valeur);
        [MethodImpl(MethodImplOptions.InternalCall)] internal static extern object[] AttributsChamp(long h);

        [MethodImpl(MethodImplOptions.InternalCall)] internal static extern string NomPropriete(long h);
        [MethodImpl(MethodImplOptions.InternalCall)] internal static extern Type TypePropriete(long h, Type proprietaire);
        [MethodImpl(MethodImplOptions.InternalCall)] internal static extern long Accesseur(long h, bool ecriture);
        [MethodImpl(MethodImplOptions.InternalCall)] internal static extern object[] AttributsPropriete(long h);

        [MethodImpl(MethodImplOptions.InternalCall)] internal static extern object Encadrer(Type t, long valeur);
        [MethodImpl(MethodImplOptions.InternalCall)] internal static extern long ValeurEntiere(object valeur);
        [MethodImpl(MethodImplOptions.InternalCall)] internal static extern object NouvelleValeur(Type t);
        [MethodImpl(MethodImplOptions.InternalCall)] internal static extern Array TableauType(Type element, object[] valeurs);

        internal static object[] Filtrer(object[] attributs, Type attributeType)
        {
            List<object> l = new List<object>();
            for (int i = 0; i < attributs.Length; i++)
                if (attributs[i] != null && attributeType.IsInstanceOfType(attributs[i])) l.Add(attributs[i]);
            return l.ToArray();
        }

        internal static bool Accepte(BindingFlags flags, bool estPublic, bool estStatique)
        {
            if (estPublic ? (flags & BindingFlags.Public) == 0 : (flags & BindingFlags.NonPublic) == 0) return false;
            if (estStatique ? (flags & BindingFlags.Static) == 0 : (flags & BindingFlags.Instance) == 0) return false;
            return true;
        }

        internal static bool MemeNom(string a, string b, BindingFlags flags)
        {
            return (flags & BindingFlags.IgnoreCase) != 0
                ? string.Equals(a, b, StringComparison.OrdinalIgnoreCase)
                : a == b;
        }

        internal const BindingFlags Defaut = BindingFlags.Public | BindingFlags.Instance | BindingFlags.Static;
    }

    public abstract class MethodBase : MemberInfo
    {
        public abstract MethodAttributes Attributes { get; }
        public abstract ParameterInfo[] GetParameters();
        public abstract object Invoke(object obj, BindingFlags invokeAttr, Binder binder, object[] parameters, CultureInfo culture);

        public object Invoke(object obj, object[] parameters)
        {
            return Invoke(obj, BindingFlags.Default, null, parameters, null);
        }

        public bool IsPublic { get { return (Attributes & MethodAttributes.MemberAccessMask) == MethodAttributes.Public; } }
        public bool IsPrivate { get { return (Attributes & MethodAttributes.MemberAccessMask) == MethodAttributes.Private; } }
        public bool IsFamily { get { return (Attributes & MethodAttributes.MemberAccessMask) == MethodAttributes.Family; } }
        public bool IsAssembly { get { return (Attributes & MethodAttributes.MemberAccessMask) == MethodAttributes.Assembly; } }
        public bool IsStatic { get { return (Attributes & MethodAttributes.Static) != 0; } }
        public bool IsVirtual { get { return (Attributes & MethodAttributes.Virtual) != 0; } }
        public bool IsAbstract { get { return (Attributes & MethodAttributes.Abstract) != 0; } }
        public bool IsFinal { get { return (Attributes & MethodAttributes.Final) != 0; } }
        public bool IsSpecialName { get { return (Attributes & MethodAttributes.SpecialName) != 0; } }
        public bool IsHideBySig { get { return (Attributes & MethodAttributes.HideBySig) != 0; } }
        public bool IsConstructor { get { return MemberType == MemberTypes.Constructor; } }
        public virtual bool IsGenericMethod { get { return false; } }
        public virtual bool IsGenericMethodDefinition { get { return false; } }
        public virtual bool ContainsGenericParameters { get { return false; } }
        public virtual Type[] GetGenericArguments() { return new Type[0]; }

        public static MethodBase GetCurrentMethod() { throw new NotSupportedException(); }
    }

    public abstract class MethodInfo : MethodBase
    {
        public override MemberTypes MemberType { get { return MemberTypes.Method; } }
        public virtual Type ReturnType { get { return null; } }
        public virtual ParameterInfo ReturnParameter { get { return null; } }
        public virtual MethodInfo GetBaseDefinition() { return this; }
        public virtual MethodInfo GetGenericMethodDefinition() { throw new InvalidOperationException(); }
        public virtual MethodInfo MakeGenericMethod(params Type[] typeArguments) { throw new NotSupportedException(); }

        public virtual Delegate CreateDelegate(Type delegateType) { throw new NotSupportedException(); }
        public virtual Delegate CreateDelegate(Type delegateType, object target) { throw new NotSupportedException(); }

        public static bool operator ==(MethodInfo left, MethodInfo right) { return (MemberInfo)left == (MemberInfo)right; }
        public static bool operator !=(MethodInfo left, MethodInfo right) { return !(left == right); }
        public override bool Equals(object obj) { return base.Equals(obj); }
        public override int GetHashCode() { return base.GetHashCode(); }
    }

    public abstract class ConstructorInfo : MethodBase
    {
        public static readonly string ConstructorName = ".ctor";
        public static readonly string TypeConstructorName = ".cctor";

        public override MemberTypes MemberType { get { return MemberTypes.Constructor; } }

        public abstract object Invoke(BindingFlags invokeAttr, Binder binder, object[] parameters, CultureInfo culture);

        public object Invoke(object[] parameters)
        {
            return Invoke(BindingFlags.Default, null, parameters, null);
        }

        public static bool operator ==(ConstructorInfo left, ConstructorInfo right) { return (MemberInfo)left == (MemberInfo)right; }
        public static bool operator !=(ConstructorInfo left, ConstructorInfo right) { return !(left == right); }
        public override bool Equals(object obj) { return base.Equals(obj); }
        public override int GetHashCode() { return base.GetHashCode(); }
    }

    public abstract class Binder { }

    public class ParameterInfo : ICustomAttributeProvider
    {
        private readonly string _nom;
        private readonly Type _type;
        private readonly int _position;
        private readonly bool _parReference;
        private readonly MemberInfo _membre;

        internal ParameterInfo(MemberInfo membre, string nom, Type type, int position, bool parReference)
        {
            _membre = membre;
            _nom = nom;
            _type = type;
            _position = position;
            _parReference = parReference;
        }

        public virtual string Name { get { return _nom; } }
        public virtual Type ParameterType { get { return _type; } }
        public virtual int Position { get { return _position; } }
        public virtual MemberInfo Member { get { return _membre; } }
        public bool IsOut { get { return _parReference; } }
        public bool IsIn { get { return false; } }
        public bool IsRetval { get { return false; } }
        public bool IsOptional { get { return false; } }
        public virtual bool HasDefaultValue { get { return false; } }
        public virtual object DefaultValue { get { return null; } }
        public virtual ParameterAttributes Attributes { get { return _parReference ? ParameterAttributes.Out : ParameterAttributes.None; } }

        public virtual object[] GetCustomAttributes(bool inherit) { return new object[0]; }
        public virtual object[] GetCustomAttributes(Type attributeType, bool inherit) { return new object[0]; }
        public virtual bool IsDefined(Type attributeType, bool inherit) { return false; }
        public override string ToString() { return _type.Name + " " + _nom; }
    }

    internal static class Appels
    {
        internal static object Invoquer(long h, Type proprietaire, object cible, object[] parametres)
        {
            try
            {
                return Rtti.Invoquer(h, proprietaire, cible, parametres ?? new object[0]);
            }
            catch (TargetParameterCountException) { throw; }
            catch (TargetException) { throw; }
            catch (Exception e)
            {
                throw new TargetInvocationException(e);
            }
        }

        internal static ParameterInfo[] Parametres(MemberInfo membre, long h, Type proprietaire)
        {
            Type[] types = Rtti.TypesParametres(h, proprietaire);
            string[] noms = Rtti.NomsParametres(h);
            long parRef = Rtti.ParametresParReference(h);
            ParameterInfo[] p = new ParameterInfo[types.Length];
            for (int i = 0; i < p.Length; i++)
                p[i] = new ParameterInfo(membre, noms[i], types[i], i, (parRef & (1L << i)) != 0);
            return p;
        }
    }

    internal sealed class RuntimeMethodInfo : MethodInfo
    {
        internal readonly long _h;
        internal readonly Type _proprietaire;

        internal RuntimeMethodInfo(long h, Type proprietaire) { _h = h; _proprietaire = proprietaire; }

        public override string Name { get { return Rtti.NomMethode(_h); } }
        public override Type DeclaringType { get { return _proprietaire; } }
        public override MethodAttributes Attributes { get { return (MethodAttributes)Rtti.DrapeauxMethode(_h); } }
        public override Type ReturnType { get { return Rtti.Retour(_h, _proprietaire); } }
        public override ParameterInfo[] GetParameters() { return Appels.Parametres(this, _h, _proprietaire); }
        public override bool IsGenericMethodDefinition { get { return Rtti.AriteMethode(_h) > 0; } }
        public override bool ContainsGenericParameters { get { return Rtti.AriteMethode(_h) > 0; } }
        internal override object[] AttributsBruts() { return Rtti.AttributsMethode(_h); }

        public override object Invoke(object obj, BindingFlags invokeAttr, Binder binder, object[] parameters, CultureInfo culture)
        {
            return Appels.Invoquer(_h, _proprietaire, obj, parameters);
        }

        public override bool Equals(object obj)
        {
            RuntimeMethodInfo m = obj as RuntimeMethodInfo;
            return m != null && m._h == _h && m._proprietaire == _proprietaire;
        }

        public override int GetHashCode() { return _h.GetHashCode(); }
        public override string ToString() { return ReturnType.Name + " " + Name; }
    }

    internal sealed class RuntimeConstructorInfo : ConstructorInfo
    {
        internal readonly long _h;
        internal readonly Type _proprietaire;

        internal RuntimeConstructorInfo(long h, Type proprietaire) { _h = h; _proprietaire = proprietaire; }

        public override string Name { get { return Rtti.NomMethode(_h); } }
        public override Type DeclaringType { get { return _proprietaire; } }
        public override MethodAttributes Attributes { get { return (MethodAttributes)Rtti.DrapeauxMethode(_h); } }
        public override ParameterInfo[] GetParameters() { return Appels.Parametres(this, _h, _proprietaire); }
        internal override object[] AttributsBruts() { return Rtti.AttributsMethode(_h); }

        public override object Invoke(BindingFlags invokeAttr, Binder binder, object[] parameters, CultureInfo culture)
        {
            return Appels.Invoquer(_h, _proprietaire, null, parameters);
        }

        public override object Invoke(object obj, BindingFlags invokeAttr, Binder binder, object[] parameters, CultureInfo culture)
        {
            return Appels.Invoquer(_h, _proprietaire, null, parameters);
        }

        public override bool Equals(object obj)
        {
            RuntimeConstructorInfo m = obj as RuntimeConstructorInfo;
            return m != null && m._h == _h && m._proprietaire == _proprietaire;
        }

        public override int GetHashCode() { return _h.GetHashCode(); }
        public override string ToString() { return "Void .ctor"; }
    }

    public abstract class FieldInfo : MemberInfo
    {
        public override MemberTypes MemberType { get { return MemberTypes.Field; } }
        public abstract FieldAttributes Attributes { get; }
        public abstract Type FieldType { get; }
        public abstract object GetValue(object obj);
        public abstract void SetValue(object obj, object value, BindingFlags invokeAttr, Binder binder, CultureInfo culture);

        public void SetValue(object obj, object value) { SetValue(obj, value, BindingFlags.Default, null, null); }
        public virtual object GetRawConstantValue() { throw new InvalidOperationException("Operation is not valid due to the current state of the object."); }

        public bool IsPublic { get { return (Attributes & FieldAttributes.FieldAccessMask) == FieldAttributes.Public; } }
        public bool IsPrivate { get { return (Attributes & FieldAttributes.FieldAccessMask) == FieldAttributes.Private; } }
        public bool IsStatic { get { return (Attributes & FieldAttributes.Static) != 0; } }
        public bool IsInitOnly { get { return (Attributes & FieldAttributes.InitOnly) != 0; } }
        public bool IsLiteral { get { return (Attributes & FieldAttributes.Literal) != 0; } }
        public bool IsSpecialName { get { return (Attributes & FieldAttributes.SpecialName) != 0; } }

        public static bool operator ==(FieldInfo left, FieldInfo right) { return (MemberInfo)left == (MemberInfo)right; }
        public static bool operator !=(FieldInfo left, FieldInfo right) { return !(left == right); }
        public override bool Equals(object obj) { return base.Equals(obj); }
        public override int GetHashCode() { return base.GetHashCode(); }
    }

    internal sealed class RuntimeFieldInfo : FieldInfo
    {
        internal readonly long _h;
        internal readonly Type _proprietaire;

        internal RuntimeFieldInfo(long h, Type proprietaire) { _h = h; _proprietaire = proprietaire; }

        public override string Name { get { return Rtti.NomChamp(_h); } }
        public override Type DeclaringType { get { return _proprietaire; } }
        public override FieldAttributes Attributes { get { return (FieldAttributes)Rtti.DrapeauxChamp(_h); } }
        public override Type FieldType { get { return Rtti.TypeChamp(_h, _proprietaire); } }
        internal override object[] AttributsBruts() { return Rtti.AttributsChamp(_h); }

        public override object GetValue(object obj)
        {
            if (IsLiteral) return Rtti.Constante(_h, _proprietaire);
            if (!IsStatic && obj == null) throw new TargetException("Non-static field requires a target.");
            return Rtti.LireChamp(_h, _proprietaire, obj);
        }

        public override object GetRawConstantValue()
        {
            if (!IsLiteral) throw new InvalidOperationException("Operation is not valid due to the current state of the object.");
            object v = Rtti.Constante(_h, _proprietaire);
            if (v is Enum) return Enum.ValeurBrute(v);
            return v;
        }

        public override void SetValue(object obj, object value, BindingFlags invokeAttr, Binder binder, CultureInfo culture)
        {
            if (IsLiteral) throw new FieldAccessException("Cannot set a constant field.");
            if (!IsStatic && obj == null) throw new TargetException("Non-static field requires a target.");
            Rtti.EcrireChamp(_h, _proprietaire, obj, value);
        }

        public override bool Equals(object obj)
        {
            RuntimeFieldInfo f = obj as RuntimeFieldInfo;
            return f != null && f._h == _h && f._proprietaire == _proprietaire;
        }

        public override int GetHashCode() { return _h.GetHashCode(); }
        public override string ToString() { return FieldType.Name + " " + Name; }
    }

    public abstract class PropertyInfo : MemberInfo
    {
        public override MemberTypes MemberType { get { return MemberTypes.Property; } }
        public abstract Type PropertyType { get; }
        public abstract bool CanRead { get; }
        public abstract bool CanWrite { get; }
        public abstract MethodInfo GetGetMethod(bool nonPublic);
        public abstract MethodInfo GetSetMethod(bool nonPublic);
        public abstract ParameterInfo[] GetIndexParameters();

        public MethodInfo GetGetMethod() { return GetGetMethod(false); }
        public MethodInfo GetSetMethod() { return GetSetMethod(false); }
        public virtual MethodInfo GetMethod { get { return GetGetMethod(true); } }
        public virtual MethodInfo SetMethod { get { return GetSetMethod(true); } }

        public MethodInfo[] GetAccessors() { return GetAccessors(false); }

        public virtual MethodInfo[] GetAccessors(bool nonPublic)
        {
            List<MethodInfo> l = new List<MethodInfo>();
            MethodInfo g = GetGetMethod(nonPublic);
            MethodInfo s = GetSetMethod(nonPublic);
            if (g != null) l.Add(g);
            if (s != null) l.Add(s);
            return l.ToArray();
        }

        public object GetValue(object obj) { return GetValue(obj, null); }

        public virtual object GetValue(object obj, object[] index)
        {
            MethodInfo g = GetGetMethod(true);
            if (g == null) throw new ArgumentException("Property get method not found.");
            return g.Invoke(obj, index ?? new object[0]);
        }

        public void SetValue(object obj, object value) { SetValue(obj, value, null); }

        public virtual void SetValue(object obj, object value, object[] index)
        {
            MethodInfo s = GetSetMethod(true);
            if (s == null) throw new ArgumentException("Property set method not found.");
            object[] args;
            if (index == null || index.Length == 0)
            {
                args = new object[] { value };
            }
            else
            {
                args = new object[index.Length + 1];
                for (int i = 0; i < index.Length; i++) args[i] = index[i];
                args[index.Length] = value;
            }
            s.Invoke(obj, args);
        }

        public static bool operator ==(PropertyInfo left, PropertyInfo right) { return (MemberInfo)left == (MemberInfo)right; }
        public static bool operator !=(PropertyInfo left, PropertyInfo right) { return !(left == right); }
        public override bool Equals(object obj) { return base.Equals(obj); }
        public override int GetHashCode() { return base.GetHashCode(); }
    }

    internal sealed class RuntimePropertyInfo : PropertyInfo
    {
        internal readonly long _h;
        internal readonly Type _proprietaire;

        internal RuntimePropertyInfo(long h, Type proprietaire) { _h = h; _proprietaire = proprietaire; }

        public override string Name { get { return Rtti.NomPropriete(_h); } }
        public override Type DeclaringType { get { return _proprietaire; } }
        public override Type PropertyType { get { return Rtti.TypePropriete(_h, _proprietaire); } }
        public override bool CanRead { get { return Rtti.Accesseur(_h, false) != 0; } }
        public override bool CanWrite { get { return Rtti.Accesseur(_h, true) != 0; } }
        internal override object[] AttributsBruts() { return Rtti.AttributsPropriete(_h); }

        private MethodInfo Accesseur(bool ecriture, bool nonPublic)
        {
            long m = Rtti.Accesseur(_h, ecriture);
            if (m == 0) return null;
            MethodInfo mi = new RuntimeMethodInfo(m, _proprietaire);
            return nonPublic || mi.IsPublic ? mi : null;
        }

        public override MethodInfo GetGetMethod(bool nonPublic) { return Accesseur(false, nonPublic); }
        public override MethodInfo GetSetMethod(bool nonPublic) { return Accesseur(true, nonPublic); }

        internal bool EstStatique
        {
            get
            {
                MethodInfo m = GetGetMethod(true) ?? GetSetMethod(true);
                return m != null && m.IsStatic;
            }
        }

        internal bool EstPublique
        {
            get
            {
                MethodInfo m = GetGetMethod(true);
                MethodInfo s = GetSetMethod(true);
                return (m != null && m.IsPublic) || (s != null && s.IsPublic);
            }
        }

        public override ParameterInfo[] GetIndexParameters()
        {
            MethodInfo g = GetGetMethod(true);
            if (g != null) return g.GetParameters();
            MethodInfo s = GetSetMethod(true);
            if (s == null) return new ParameterInfo[0];
            ParameterInfo[] p = s.GetParameters();
            ParameterInfo[] r = new ParameterInfo[p.Length - 1];
            for (int i = 0; i < r.Length; i++) r[i] = p[i];
            return r;
        }

        public override bool Equals(object obj)
        {
            RuntimePropertyInfo p = obj as RuntimePropertyInfo;
            return p != null && p._h == _h && p._proprietaire == _proprietaire;
        }

        public override int GetHashCode() { return _h.GetHashCode(); }
        public override string ToString() { return PropertyType.Name + " " + Name; }
    }

    public static class CustomAttributeExtensions
    {
        public static T GetCustomAttribute<T>(this MemberInfo element) where T : Attribute
        {
            return (T)Attribute.GetCustomAttribute(element, typeof(T));
        }

        public static T GetCustomAttribute<T>(this MemberInfo element, bool inherit) where T : Attribute
        {
            return (T)Attribute.GetCustomAttribute(element, typeof(T), inherit);
        }

        public static T GetCustomAttribute<T>(this Assembly element) where T : Attribute
        {
            return (T)Attribute.GetCustomAttribute(element, typeof(T));
        }

        public static IEnumerable<T> GetCustomAttributes<T>(this MemberInfo element) where T : Attribute
        {
            object[] a = element.GetCustomAttributes(typeof(T), true);
            List<T> l = new List<T>();
            for (int i = 0; i < a.Length; i++) l.Add((T)a[i]);
            return l;
        }

        public static IEnumerable<Attribute> GetCustomAttributes(this MemberInfo element)
        {
            object[] a = element.GetCustomAttributes(true);
            List<Attribute> l = new List<Attribute>();
            for (int i = 0; i < a.Length; i++) l.Add((Attribute)a[i]);
            return l;
        }

        public static bool IsDefined(this MemberInfo element, Type attributeType)
        {
            return element.IsDefined(attributeType, true);
        }
    }

    public abstract partial class Assembly : ICustomAttributeProvider
    {
        private long Indice { get { return Rtti.AssemblyDe(this); } }

        public virtual object[] GetCustomAttributes(bool inherit) { return Rtti.AttributsAssembly(Indice); }

        public virtual object[] GetCustomAttributes(Type attributeType, bool inherit)
        {
            if ((object)attributeType == null) throw new ArgumentNullException("attributeType");
            return Rtti.Filtrer(GetCustomAttributes(inherit), attributeType);
        }

        public virtual bool IsDefined(Type attributeType, bool inherit)
        {
            return GetCustomAttributes(attributeType, inherit).Length != 0;
        }

        public virtual Type GetType(string name) { return GetType(name, false, false); }
        public virtual Type GetType(string name, bool throwOnError) { return GetType(name, throwOnError, false); }

        public virtual Type GetType(string name, bool throwOnError, bool ignoreCase)
        {
            if (name == null) throw new ArgumentNullException("name");
            Type t = Rtti.TypeDansAssembly(Indice, name, ignoreCase);
            if ((object)t == null && throwOnError)
                throw new TypeLoadException("Could not load type '" + name + "' from assembly '" + FullName + "'.");
            return t;
        }

        public virtual Type[] GetTypes() { return Rtti.TypesAssembly(Indice); }
        public virtual Type[] GetExportedTypes()
        {
            Type[] tous = GetTypes();
            List<Type> l = new List<Type>();
            for (int i = 0; i < tous.Length; i++) if (tous[i].IsPublic) l.Add(tous[i]);
            return l.ToArray();
        }

        public static Assembly Load(string assemblyString)
        {
            if (assemblyString == null) throw new ArgumentNullException("assemblyString");
            Assembly a = Rtti.AssemblyParNom(assemblyString);
            if (a == null)
                throw new System.IO.FileNotFoundException("Could not load file or assembly '" + assemblyString + "' or one of its dependencies. The system cannot find the file specified.");
            return a;
        }

        public static Assembly Load(AssemblyName assemblyRef)
        {
            if (assemblyRef == null) throw new ArgumentNullException("assemblyRef");
            return Load(assemblyRef.Name);
        }
    }
}

namespace System
{
    public class FieldAccessException : MemberAccessException
    {
        public FieldAccessException() : base("Attempted to access a field that is not accessible by the caller.") { }
        public FieldAccessException(string message) : base(message) { }
    }

    public class MemberAccessException : SystemException
    {
        public MemberAccessException() : base("Cannot access member.") { }
        public MemberAccessException(string message) : base(message) { }
    }

    public enum TypeCode
    {
        Empty = 0,
        Object = 1,
        DBNull = 2,
        Boolean = 3,
        Char = 4,
        SByte = 5,
        Byte = 6,
        Int16 = 7,
        UInt16 = 8,
        Int32 = 9,
        UInt32 = 10,
        Int64 = 11,
        UInt64 = 12,
        Single = 13,
        Double = 14,
        Decimal = 15,
        DateTime = 16,
        String = 18,
    }

    public abstract partial class Type
    {
        public static readonly Type[] EmptyTypes = new Type[0];
        public static readonly object Missing = new object();

        public override MemberTypes MemberType { get { return MemberTypes.TypeInfo; } }
        internal override object[] AttributsBruts() { return Rtti.AttributsType(this); }

        public override object[] GetCustomAttributes(bool inherit)
        {
            object[] propres = Rtti.AttributsType(this);
            if (!inherit || (object)BaseType == null) return propres;
            // Attributs hérités (AttributeUsage.Inherited vaut vrai par
            // défaut) : ceux des types parents s'ajoutent.
            List<object> l = new List<object>(propres);
            for (Type t = BaseType; (object)t != null; t = t.BaseType) l.AddRange(Rtti.AttributsType(t));
            return l.ToArray();
        }

        public TypeAttributes Attributes { get { return (TypeAttributes)Rtti.DrapeauxType(this); } }
        public bool IsAbstract { get { return (Attributes & TypeAttributes.Abstract) != 0; } }
        public bool IsSealed { get { return (Attributes & TypeAttributes.Sealed) != 0; } }
        public bool IsPublic { get { return (Attributes & TypeAttributes.VisibilityMask) == TypeAttributes.Public; } }
        public bool IsNotPublic { get { return (Attributes & TypeAttributes.VisibilityMask) == TypeAttributes.NotPublic; } }
        public bool IsNestedPublic { get { return (Attributes & TypeAttributes.VisibilityMask) == TypeAttributes.NestedPublic; } }
        public bool IsNested { get { return (int)(Attributes & TypeAttributes.VisibilityMask) > 1; } }
        public bool IsSerializable { get { return (Attributes & TypeAttributes.Serializable) != 0; } }
        public bool IsArray { get { return this == typeof(Array) || IsSubclassOf(typeof(Array)); } }
        public bool IsByRef { get { return false; } }
        public bool IsPointer { get { return false; } }
        public bool IsCOMObject { get { return false; } }
        public virtual bool IsGenericParameter { get { return false; } }
        public bool HasElementType { get { return false; } }
        public virtual Type GetElementType() { return null; }
        public virtual Type UnderlyingSystemType { get { return this; } }

        public bool IsPrimitive
        {
            get
            {
                return this == typeof(bool) || this == typeof(char) || this == typeof(sbyte) || this == typeof(byte)
                    || this == typeof(short) || this == typeof(ushort) || this == typeof(int) || this == typeof(uint)
                    || this == typeof(long) || this == typeof(ulong) || this == typeof(float) || this == typeof(double)
                    || this == typeof(IntPtr) || this == typeof(UIntPtr);
            }
        }

        public virtual bool IsGenericType { get { return Rtti.ArgumentsGeneriques(this).Length != 0 || Rtti.AriteGenerique(this) != 0; } }
        public virtual bool IsGenericTypeDefinition { get { return Rtti.AriteGenerique(this) != 0 && Rtti.ArgumentsGeneriques(this).Length == 0; } }
        public virtual bool IsConstructedGenericType { get { return Rtti.ArgumentsGeneriques(this).Length != 0; } }
        public virtual bool ContainsGenericParameters { get { return IsGenericTypeDefinition; } }
        public virtual Type[] GetGenericArguments() { return Rtti.ArgumentsGeneriques(this); }
        public virtual Type[] GenericTypeArguments { get { return Rtti.ArgumentsGeneriques(this); } }

        public virtual Type GetGenericTypeDefinition()
        {
            if (IsGenericTypeDefinition) return this;
            Type d = Rtti.DefinitionGenerique(this);
            if ((object)d == null) throw new InvalidOperationException("This operation is only valid on generic types.");
            return d;
        }

        public virtual Type MakeGenericType(params Type[] typeArguments)
        {
            if (typeArguments == null) throw new ArgumentNullException("typeArguments");
            if (!IsGenericTypeDefinition) throw new InvalidOperationException(FullName + " is not a GenericTypeDefinition. MakeGenericType may only be called on a type for which Type.IsGenericTypeDefinition is true.");
            if (typeArguments.Length != Rtti.AriteGenerique(this)) throw new ArgumentException("The number of generic arguments provided doesn't equal the arity of the generic type definition.", "typeArguments");
            return Rtti.Instancier(this, typeArguments);
        }

        public virtual bool IsAssignableFrom(Type c)
        {
            if ((object)c == null) return false;
            return Rtti.Assignable(this, c);
        }

        public virtual bool IsSubclassOf(Type c)
        {
            if ((object)c == null) throw new ArgumentNullException("c");
            for (Type t = BaseType; (object)t != null; t = t.BaseType)
                if (t == c) return true;
            return false;
        }

        public virtual bool IsInstanceOfType(object o)
        {
            return o != null && IsAssignableFrom(o.GetType());
        }

        public virtual Type[] GetInterfaces() { return Rtti.Interfaces(this); }

        public Type GetInterface(string name) { return GetInterface(name, false); }

        public virtual Type GetInterface(string name, bool ignoreCase)
        {
            if (name == null) throw new ArgumentNullException("name");
            Type[] i = GetInterfaces();
            for (int k = 0; k < i.Length; k++)
            {
                StringComparison c = ignoreCase ? StringComparison.OrdinalIgnoreCase : StringComparison.Ordinal;
                if (string.Equals(i[k].Name, name, c) || string.Equals(i[k].FullName, name, c)) return i[k];
            }
            return null;
        }

        // --- Membres ---

        private List<MethodBase> MethodesEtConstructeurs(BindingFlags flags, bool constructeurs)
        {
            List<MethodBase> l = new List<MethodBase>();
            bool premier = true;
            for (Type t = this; (object)t != null; t = t.BaseType)
            {
                if (!premier && (constructeurs || (flags & BindingFlags.DeclaredOnly) != 0)) break;
                long[] m = Rtti.Methodes(t);
                for (int i = 0; i < m.Length; i++)
                {
                    string nom = Rtti.NomMethode(m[i]);
                    bool ctor = nom == ".ctor";
                    if (nom == ".cctor" || ctor != constructeurs) continue;
                    MethodAttributes a = (MethodAttributes)Rtti.DrapeauxMethode(m[i]);
                    bool statique = (a & MethodAttributes.Static) != 0;
                    bool publique = (a & MethodAttributes.MemberAccessMask) == MethodAttributes.Public;
                    if (!Rtti.Accepte(flags, publique, statique)) continue;
                    // Membres privés et statiques d'un parent : invisibles,
                    // sauf FlattenHierarchy pour les statiques publics.
                    if (!premier && ((a & MethodAttributes.MemberAccessMask) == MethodAttributes.Private)) continue;
                    if (!premier && statique && (flags & BindingFlags.FlattenHierarchy) == 0) continue;
                    if (!premier && (a & MethodAttributes.Virtual) != 0 && Redefinie(l, nom, m[i], t)) continue;
                    if (ctor) l.Add(new RuntimeConstructorInfo(m[i], t));
                    else l.Add(new RuntimeMethodInfo(m[i], t));
                }
                premier = false;
            }
            return l;
        }

        // Une méthode virtuelle d'un parent que le type redéfinit n'apparaît
        // qu'une fois, sous la redéfinition.
        private static bool Redefinie(List<MethodBase> deja, string nom, long h, Type proprietaire)
        {
            int n = Rtti.TypesParametres(h, proprietaire).Length;
            for (int i = 0; i < deja.Count; i++)
                if (deja[i].Name == nom && deja[i].IsVirtual && deja[i].GetParameters().Length == n) return true;
            return false;
        }

        public MethodInfo[] GetMethods() { return GetMethods(BindingFlags.Public | BindingFlags.Instance | BindingFlags.Static); }

        public virtual MethodInfo[] GetMethods(BindingFlags bindingAttr)
        {
            List<MethodBase> l = MethodesEtConstructeurs(bindingAttr, false);
            MethodInfo[] r = new MethodInfo[l.Count];
            for (int i = 0; i < r.Length; i++) r[i] = (MethodInfo)l[i];
            return r;
        }

        public MethodInfo GetMethod(string name) { return GetMethod(name, Rtti.Defaut); }

        public MethodInfo GetMethod(string name, BindingFlags bindingAttr)
        {
            if (name == null) throw new ArgumentNullException("name");
            MethodInfo trouve = null;
            MethodInfo[] m = GetMethods(bindingAttr);
            for (int i = 0; i < m.Length; i++)
            {
                if (!Rtti.MemeNom(m[i].Name, name, bindingAttr)) continue;
                if (trouve != null) throw new AmbiguousMatchException();
                trouve = m[i];
            }
            return trouve;
        }

        public MethodInfo GetMethod(string name, Type[] types) { return GetMethod(name, Rtti.Defaut, null, types, null); }

        public MethodInfo GetMethod(string name, BindingFlags bindingAttr, Binder binder, Type[] types, ParameterModifier[] modifiers)
        {
            if (name == null) throw new ArgumentNullException("name");
            if (types == null) throw new ArgumentNullException("types");
            MethodInfo[] m = GetMethods(bindingAttr);
            for (int i = 0; i < m.Length; i++)
                if (Rtti.MemeNom(m[i].Name, name, bindingAttr) && MemesParametres(m[i], types)) return m[i];
            return null;
        }

        public MethodInfo GetMethod(string name, BindingFlags bindingAttr, Binder binder, CallingConventions callConvention, Type[] types, ParameterModifier[] modifiers)
        {
            return GetMethod(name, bindingAttr, binder, types, modifiers);
        }

        private static bool MemesParametres(MethodBase m, Type[] types)
        {
            ParameterInfo[] p = m.GetParameters();
            if (p.Length != types.Length) return false;
            for (int i = 0; i < p.Length; i++)
                if (p[i].ParameterType != types[i]) return false;
            return true;
        }

        public ConstructorInfo[] GetConstructors() { return GetConstructors(BindingFlags.Public | BindingFlags.Instance); }

        public virtual ConstructorInfo[] GetConstructors(BindingFlags bindingAttr)
        {
            List<MethodBase> l = MethodesEtConstructeurs(bindingAttr, true);
            ConstructorInfo[] r = new ConstructorInfo[l.Count];
            for (int i = 0; i < r.Length; i++) r[i] = (ConstructorInfo)l[i];
            return r;
        }

        public ConstructorInfo GetConstructor(Type[] types)
        {
            return GetConstructor(BindingFlags.Public | BindingFlags.Instance, null, types, null);
        }

        public ConstructorInfo GetConstructor(BindingFlags bindingAttr, Binder binder, Type[] types, ParameterModifier[] modifiers)
        {
            if (types == null) throw new ArgumentNullException("types");
            ConstructorInfo[] c = GetConstructors(bindingAttr);
            for (int i = 0; i < c.Length; i++)
                if (MemesParametres(c[i], types)) return c[i];
            return null;
        }

        public FieldInfo[] GetFields() { return GetFields(BindingFlags.Public | BindingFlags.Instance | BindingFlags.Static); }

        public virtual FieldInfo[] GetFields(BindingFlags bindingAttr)
        {
            List<FieldInfo> l = new List<FieldInfo>();
            bool premier = true;
            for (Type t = this; (object)t != null; t = t.BaseType)
            {
                if (!premier && (bindingAttr & BindingFlags.DeclaredOnly) != 0) break;
                long[] f = Rtti.Champs(t);
                for (int i = 0; i < f.Length; i++)
                {
                    FieldAttributes a = (FieldAttributes)Rtti.DrapeauxChamp(f[i]);
                    bool statique = (a & FieldAttributes.Static) != 0;
                    bool publique = (a & FieldAttributes.FieldAccessMask) == FieldAttributes.Public;
                    if (!Rtti.Accepte(bindingAttr, publique, statique)) continue;
                    if (!premier && (a & FieldAttributes.FieldAccessMask) == FieldAttributes.Private) continue;
                    if (!premier && statique && (bindingAttr & BindingFlags.FlattenHierarchy) == 0) continue;
                    l.Add(new RuntimeFieldInfo(f[i], t));
                }
                premier = false;
            }
            return l.ToArray();
        }

        public FieldInfo GetField(string name) { return GetField(name, Rtti.Defaut); }

        public virtual FieldInfo GetField(string name, BindingFlags bindingAttr)
        {
            if (name == null) throw new ArgumentNullException("name");
            FieldInfo[] f = GetFields(bindingAttr);
            for (int i = 0; i < f.Length; i++)
                if (Rtti.MemeNom(f[i].Name, name, bindingAttr)) return f[i];
            return null;
        }

        public PropertyInfo[] GetProperties() { return GetProperties(BindingFlags.Public | BindingFlags.Instance | BindingFlags.Static); }

        public virtual PropertyInfo[] GetProperties(BindingFlags bindingAttr)
        {
            List<PropertyInfo> l = new List<PropertyInfo>();
            bool premier = true;
            for (Type t = this; (object)t != null; t = t.BaseType)
            {
                if (!premier && (bindingAttr & BindingFlags.DeclaredOnly) != 0) break;
                long[] p = Rtti.Proprietes(t);
                for (int i = 0; i < p.Length; i++)
                {
                    RuntimePropertyInfo pi = new RuntimePropertyInfo(p[i], t);
                    if (!Rtti.Accepte(bindingAttr, pi.EstPublique, pi.EstStatique)) continue;
                    bool deja = false;
                    for (int k = 0; k < l.Count; k++) if (l[k].Name == pi.Name) { deja = true; break; }
                    if (deja) continue;
                    if (!premier && pi.EstStatique && (bindingAttr & BindingFlags.FlattenHierarchy) == 0) continue;
                    l.Add(pi);
                }
                premier = false;
            }
            return l.ToArray();
        }

        public PropertyInfo GetProperty(string name) { return GetProperty(name, Rtti.Defaut); }

        public virtual PropertyInfo GetProperty(string name, BindingFlags bindingAttr)
        {
            if (name == null) throw new ArgumentNullException("name");
            PropertyInfo trouve = null;
            PropertyInfo[] p = GetProperties(bindingAttr);
            for (int i = 0; i < p.Length; i++)
            {
                if (!Rtti.MemeNom(p[i].Name, name, bindingAttr)) continue;
                if (trouve != null) throw new AmbiguousMatchException();
                trouve = p[i];
            }
            return trouve;
        }

        public PropertyInfo GetProperty(string name, Type returnType)
        {
            PropertyInfo[] p = GetProperties(Rtti.Defaut);
            for (int i = 0; i < p.Length; i++)
                if (p[i].Name == name && ((object)returnType == null || p[i].PropertyType == returnType)) return p[i];
            return null;
        }

        public PropertyInfo GetProperty(string name, Type returnType, Type[] types)
        {
            PropertyInfo[] p = GetProperties(Rtti.Defaut);
            for (int i = 0; i < p.Length; i++)
            {
                if (p[i].Name != name || ((object)returnType != null && p[i].PropertyType != returnType)) continue;
                ParameterInfo[] idx = p[i].GetIndexParameters();
                if (idx.Length != types.Length) continue;
                bool ok = true;
                for (int k = 0; k < idx.Length; k++) if (idx[k].ParameterType != types[k]) { ok = false; break; }
                if (ok) return p[i];
            }
            return null;
        }

        public MemberInfo[] GetMember(string name) { return GetMember(name, Rtti.Defaut); }

        public virtual MemberInfo[] GetMember(string name, BindingFlags bindingAttr)
        {
            if (name == null) throw new ArgumentNullException("name");
            List<MemberInfo> l = new List<MemberInfo>();
            MemberInfo[] tous = GetMembers(bindingAttr);
            for (int i = 0; i < tous.Length; i++)
                if (Rtti.MemeNom(tous[i].Name, name, bindingAttr)) l.Add(tous[i]);
            return l.ToArray();
        }

        public MemberInfo[] GetMembers() { return GetMembers(BindingFlags.Public | BindingFlags.Instance | BindingFlags.Static); }

        public virtual MemberInfo[] GetMembers(BindingFlags bindingAttr)
        {
            List<MemberInfo> l = new List<MemberInfo>();
            l.AddRange(GetMethods(bindingAttr));
            l.AddRange(GetConstructors(bindingAttr));
            l.AddRange(GetProperties(bindingAttr));
            l.AddRange(GetFields(bindingAttr));
            return l.ToArray();
        }

        public object InvokeMember(string name, BindingFlags invokeAttr, Binder binder, object target, object[] args)
        {
            if ((invokeAttr & BindingFlags.GetProperty) != 0) return GetProperty(name, invokeAttr | Rtti.Defaut).GetValue(target, null);
            if ((invokeAttr & BindingFlags.SetProperty) != 0) { GetProperty(name, invokeAttr | Rtti.Defaut).SetValue(target, args[0], null); return null; }
            if ((invokeAttr & BindingFlags.GetField) != 0) return GetField(name, invokeAttr | Rtti.Defaut).GetValue(target);
            if ((invokeAttr & BindingFlags.SetField) != 0) { GetField(name, invokeAttr | Rtti.Defaut).SetValue(target, args[0]); return null; }
            if ((invokeAttr & BindingFlags.CreateInstance) != 0) return Activator.CreateInstance(this, args);
            MethodInfo[] m = GetMethods(invokeAttr | Rtti.Defaut);
            int n = args == null ? 0 : args.Length;
            for (int i = 0; i < m.Length; i++)
                if (Rtti.MemeNom(m[i].Name, name, invokeAttr) && m[i].GetParameters().Length == n) return m[i].Invoke(target, args);
            throw new MissingMethodException("Method '" + FullName + "." + name + "' not found.");
        }

        // --- Énumérations ---

        public virtual Type GetEnumUnderlyingType()
        {
            if (!IsEnum) throw new ArgumentException("Type provided must be an Enum.", "enumType");
            FieldInfo[] f = GetFields(BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.Instance);
            return f.Length > 0 ? f[0].FieldType : typeof(int);
        }

        public virtual string[] GetEnumNames() { return Enum.GetNames(this); }
        public virtual Array GetEnumValues() { return Enum.GetValues(this); }

        public static TypeCode GetTypeCode(Type type)
        {
            if ((object)type == null) return TypeCode.Empty;
            if (type.IsEnum) type = type.GetEnumUnderlyingType();
            if (type == typeof(bool)) return TypeCode.Boolean;
            if (type == typeof(char)) return TypeCode.Char;
            if (type == typeof(sbyte)) return TypeCode.SByte;
            if (type == typeof(byte)) return TypeCode.Byte;
            if (type == typeof(short)) return TypeCode.Int16;
            if (type == typeof(ushort)) return TypeCode.UInt16;
            if (type == typeof(int)) return TypeCode.Int32;
            if (type == typeof(uint)) return TypeCode.UInt32;
            if (type == typeof(long)) return TypeCode.Int64;
            if (type == typeof(ulong)) return TypeCode.UInt64;
            if (type == typeof(float)) return TypeCode.Single;
            if (type == typeof(double)) return TypeCode.Double;
            if (type == typeof(DateTime)) return TypeCode.DateTime;
            if (type == typeof(string)) return TypeCode.String;
            return TypeCode.Object;
        }
    }

    public partial class Attribute
    {
        public virtual object TypeId { get { return GetType(); } }

        public static Attribute GetCustomAttribute(MemberInfo element, Type attributeType) { return GetCustomAttribute(element, attributeType, true); }

        public static Attribute GetCustomAttribute(MemberInfo element, Type attributeType, bool inherit)
        {
            if (element == null) throw new ArgumentNullException("element");
            return Unique(element.GetCustomAttributes(attributeType, inherit));
        }

        public static Attribute GetCustomAttribute(Assembly element, Type attributeType)
        {
            if (element == null) throw new ArgumentNullException("element");
            return Unique(element.GetCustomAttributes(attributeType, true));
        }

        public static Attribute[] GetCustomAttributes(MemberInfo element, Type type) { return GetCustomAttributes(element, type, true); }

        public static Attribute[] GetCustomAttributes(MemberInfo element, Type type, bool inherit)
        {
            if (element == null) throw new ArgumentNullException("element");
            return EnAttributs(element.GetCustomAttributes(type, inherit));
        }

        public static Attribute[] GetCustomAttributes(MemberInfo element) { return EnAttributs(element.GetCustomAttributes(true)); }

        public static Attribute[] GetCustomAttributes(Assembly element, Type attributeType)
        {
            return EnAttributs(element.GetCustomAttributes(attributeType, true));
        }

        public static bool IsDefined(MemberInfo element, Type attributeType) { return element.IsDefined(attributeType, true); }
        public static bool IsDefined(MemberInfo element, Type attributeType, bool inherit) { return element.IsDefined(attributeType, inherit); }
        public static bool IsDefined(Assembly element, Type attributeType) { return element.IsDefined(attributeType, true); }

        private static Attribute Unique(object[] a)
        {
            if (a.Length == 0) return null;
            if (a.Length > 1) throw new AmbiguousMatchException("Multiple custom attributes of the same type found.");
            return (Attribute)a[0];
        }

        private static Attribute[] EnAttributs(object[] a)
        {
            Attribute[] r = new Attribute[a.Length];
            for (int i = 0; i < a.Length; i++) r[i] = (Attribute)a[i];
            return r;
        }
    }

    public static class Activator
    {
        public static object CreateInstance(Type type) { return CreateInstance(type, false); }

        public static object CreateInstance(Type type, bool nonPublic)
        {
            if ((object)type == null) throw new ArgumentNullException("type");
            BindingFlags f = BindingFlags.Instance | BindingFlags.Public | (nonPublic ? BindingFlags.NonPublic : 0);
            ConstructorInfo c = type.GetConstructor(f, null, Type.EmptyTypes, null);
            if (c == null)
            {
                if (type.IsValueType) return Rtti.NouvelleValeur(type);
                throw new MissingMethodException("No parameterless constructor defined for this object.");
            }
            return c.Invoke(new object[0]);
        }

        public static object CreateInstance(Type type, params object[] args)
        {
            if ((object)type == null) throw new ArgumentNullException("type");
            if (args == null || args.Length == 0) return CreateInstance(type, false);
            ConstructorInfo[] c = type.GetConstructors(BindingFlags.Instance | BindingFlags.Public);
            for (int i = 0; i < c.Length; i++)
            {
                ParameterInfo[] p = c[i].GetParameters();
                if (p.Length != args.Length) continue;
                bool ok = true;
                for (int k = 0; k < p.Length && ok; k++)
                    ok = args[k] == null ? !p[k].ParameterType.IsValueType : p[k].ParameterType.IsInstanceOfType(args[k]);
                if (ok) return c[i].Invoke(args);
            }
            throw new MissingMethodException("Constructor on type '" + type.FullName + "' not found.");
        }

        public static object CreateInstance(Type type, BindingFlags bindingAttr, Binder binder, object[] args, CultureInfo culture)
        {
            return CreateInstance(type, args);
        }

        public static T CreateInstance<T>() { return (T)CreateInstance(typeof(T), true); }
    }

    public abstract partial class Enum
    {
        internal static object ValeurBrute(object v)
        {
            Type sous = v.GetType().GetEnumUnderlyingType();
            long x = Rtti.ValeurEntiere(v);
            return Rtti.Encadrer(sous, x);
        }

        private static void Verifier(Type enumType)
        {
            if ((object)enumType == null) throw new ArgumentNullException("enumType");
            if (!enumType.IsEnum) throw new ArgumentException("Type provided must be an Enum.", "enumType");
        }

        private static FieldInfo[] Membres(Type enumType)
        {
            Verifier(enumType);
            return enumType.GetFields(BindingFlags.Public | BindingFlags.Static);
        }

        public static Type GetUnderlyingType(Type enumType)
        {
            Verifier(enumType);
            return enumType.GetEnumUnderlyingType();
        }

        public static Array GetValues(Type enumType)
        {
            object[] objets = ValeursTriees(enumType);
            return Rtti.TableauType(enumType, objets);
        }

        private static object[] ValeursTriees(Type enumType)
        {
            FieldInfo[] f = Membres(enumType);
            long[] valeurs = new long[f.Length];
            object[] objets = new object[f.Length];
            for (int i = 0; i < f.Length; i++)
            {
                objets[i] = f[i].GetValue(null);
                valeurs[i] = Rtti.ValeurEntiere(objets[i]);
            }
            // Triées par valeur non signée, comme le documente GetValues.
            for (int i = 1; i < f.Length; i++)
            {
                for (int k = i; k > 0 && (ulong)valeurs[k - 1] > (ulong)valeurs[k]; k--)
                {
                    long tv = valeurs[k]; valeurs[k] = valeurs[k - 1]; valeurs[k - 1] = tv;
                    object to = objets[k]; objets[k] = objets[k - 1]; objets[k - 1] = to;
                }
            }
            return objets;
        }

        public static string[] GetNames(Type enumType)
        {
            object[] v = ValeursTriees(enumType);
            string[] n = new string[v.Length];
            for (int i = 0; i < n.Length; i++) n[i] = GetName(enumType, v[i]);
            return n;
        }

        public static string GetName(Type enumType, object value)
        {
            if (value == null) throw new ArgumentNullException("value");
            long x = Rtti.ValeurEntiere(value);
            FieldInfo[] f = Membres(enumType);
            for (int i = 0; i < f.Length; i++)
                if (Rtti.ValeurEntiere(f[i].GetValue(null)) == x) return f[i].Name;
            return null;
        }

        public static bool IsDefined(Type enumType, object value)
        {
            if (value == null) throw new ArgumentNullException("value");
            string s = value as string;
            if (s != null)
            {
                FieldInfo[] f = Membres(enumType);
                for (int i = 0; i < f.Length; i++) if (f[i].Name == s) return true;
                return false;
            }
            return GetName(enumType, value) != null;
        }

        public static object ToObject(Type enumType, object value)
        {
            if (value == null) throw new ArgumentNullException("value");
            return ToObject(enumType, Rtti.ValeurEntiere(value));
        }

        public static object ToObject(Type enumType, int value) { return ToObject(enumType, (long)value); }
        public static object ToObject(Type enumType, byte value) { return ToObject(enumType, (long)value); }
        public static object ToObject(Type enumType, short value) { return ToObject(enumType, (long)value); }
        public static object ToObject(Type enumType, uint value) { return ToObject(enumType, (long)value); }
        public static object ToObject(Type enumType, ulong value) { return ToObject(enumType, (long)value); }

        public static object ToObject(Type enumType, long value)
        {
            Verifier(enumType);
            return Rtti.Encadrer(enumType, value);
        }

        public static object Parse(Type enumType, string value) { return Parse(enumType, value, false); }

        public static object Parse(Type enumType, string value, bool ignoreCase)
        {
            object r;
            if (!Essayer(enumType, value, ignoreCase, out r))
                throw new ArgumentException("Requested value '" + value + "' was not found.");
            return r;
        }

        public static bool TryParse<TEnum>(string value, out TEnum result) where TEnum : struct
        {
            return TryParse(value, false, out result);
        }

        public static bool TryParse<TEnum>(string value, bool ignoreCase, out TEnum result) where TEnum : struct
        {
            object r;
            if (value != null && Essayer(typeof(TEnum), value, ignoreCase, out r))
            {
                result = (TEnum)r;
                return true;
            }
            result = default(TEnum);
            return false;
        }

        // « A », « A, B » (drapeaux) ou une valeur numérique.
        private static bool Essayer(Type enumType, string value, bool ignoreCase, out object result)
        {
            Verifier(enumType);
            if (value == null) throw new ArgumentNullException("value");
            result = null;
            string t = value.Trim();
            if (t.Length == 0) return false;
            long n;
            if (char.IsDigit(t[0]) || t[0] == '-' || t[0] == '+')
            {
                if (!long.TryParse(t, out n)) return false;
                result = ToObject(enumType, n);
                return true;
            }
            FieldInfo[] f = Membres(enumType);
            long total = 0;
            string[] parts = t.Split(',');
            for (int p = 0; p < parts.Length; p++)
            {
                string nom = parts[p].Trim();
                bool trouve = false;
                for (int i = 0; i < f.Length; i++)
                {
                    if (string.Equals(f[i].Name, nom, ignoreCase ? StringComparison.OrdinalIgnoreCase : StringComparison.Ordinal))
                    {
                        total |= Rtti.ValeurEntiere(f[i].GetValue(null));
                        trouve = true;
                        break;
                    }
                }
                if (!trouve) return false;
            }
            result = ToObject(enumType, total);
            return true;
        }

        public bool HasFlag(Enum flag)
        {
            if (flag == null) throw new ArgumentNullException("flag");
            long f = Rtti.ValeurEntiere(flag);
            return (Rtti.ValeurEntiere(this) & f) == f;
        }

        // Nom de la valeur, pas celui du type (#2010).
        public override string ToString() { return Texte(GetType(), Rtti.ValeurEntiere(this), false); }

        public string ToString(string format)
        {
            if (string.IsNullOrEmpty(format) || format == "G" || format == "g") return ToString();
            long v = Rtti.ValeurEntiere(this);
            if (format == "D" || format == "d") return v.ToString();
            if (format == "F" || format == "f") return Texte(GetType(), v, true);
            if (format == "X" || format == "x")
            {
                int octets = Taille(GetType());
                ulong masque = octets >= 8 ? ulong.MaxValue : (1UL << (octets * 8)) - 1;
                return ((ulong)v & masque).ToString("X" + (octets * 2));
            }
            throw new FormatException("Format String can be only \"G\", \"g\", \"X\", \"x\", \"F\", \"f\", \"D\" or \"d\".");
        }

        public string ToString(IFormatProvider provider) { return ToString(); }

        private static int Taille(Type enumType)
        {
            Type s = enumType.GetEnumUnderlyingType();
            if (s == typeof(byte) || s == typeof(sbyte)) return 1;
            if (s == typeof(short) || s == typeof(ushort)) return 2;
            if (s == typeof(long) || s == typeof(ulong)) return 8;
            return 4;
        }

        // Nom de la valeur ; pour [Flags] (ou « F »), noms séparés par
        // « , » ; sinon la valeur numérique.
        private static string Texte(Type enumType, long v, bool forcerDrapeaux)
        {
            object[] valeurs = ValeursTriees(enumType);
            for (int i = 0; i < valeurs.Length; i++)
                if (Rtti.ValeurEntiere(valeurs[i]) == v) return NomDe(enumType, valeurs[i]);
            if (forcerDrapeaux || enumType.IsDefined(typeof(FlagsAttribute), false))
            {
                long reste = v;
                List<string> noms = new List<string>();
                for (int i = valeurs.Length - 1; i >= 0; i--)
                {
                    long x = Rtti.ValeurEntiere(valeurs[i]);
                    if (x != 0 && (reste & x) == x)
                    {
                        noms.Insert(0, NomDe(enumType, valeurs[i]));
                        reste &= ~x;
                    }
                }
                if (reste == 0 && noms.Count > 0) return string.Join(", ", noms.ToArray());
            }
            return v.ToString();
        }

        private static string NomDe(Type enumType, object valeur)
        {
            long x = Rtti.ValeurEntiere(valeur);
            FieldInfo[] f = Membres(enumType);
            for (int i = 0; i < f.Length; i++)
                if (Rtti.ValeurEntiere(f[i].GetValue(null)) == x) return f[i].Name;
            return x.ToString();
        }

        public TypeCode GetTypeCode() { return Type.GetTypeCode(GetType()); }

        public static string Format(Type enumType, object value, string format)
        {
            return ((Enum)ToObject(enumType, value)).ToString(format);
        }
    }
}

namespace System.Reflection
{
    public struct ParameterModifier
    {
        private bool[] _parRef;
        public ParameterModifier(int parameterCount) { _parRef = new bool[parameterCount]; }
        public bool this[int index] { get { return _parRef[index]; } set { _parRef[index] = value; } }
    }

    [Flags]
    public enum CallingConventions
    {
        Standard = 1,
        VarArgs = 2,
        Any = 3,
        HasThis = 32,
        ExplicitThis = 64,
    }
}

namespace System.Globalization
{
    // Culture minimale : nom et cultures invariante et courante, pour les
    // signatures qui la demandent (MethodBase.Invoke…). Les formats
    // restent ceux de la culture invariante.
    public class CultureInfo : IFormatProvider
    {
        private readonly string _nom;
        private static readonly CultureInfo s_invariante = new CultureInfo("");

        public CultureInfo(string name)
        {
            if (name == null) throw new ArgumentNullException("name");
            _nom = name;
        }

        public CultureInfo(int culture) : this(culture == 0x7F ? "" : "en-US") { }

        public static CultureInfo InvariantCulture { get { return s_invariante; } }
        public static CultureInfo CurrentCulture { get { return s_invariante; } }
        public static CultureInfo CurrentUICulture { get { return s_invariante; } }
        public virtual string Name { get { return _nom; } }
        public virtual string TwoLetterISOLanguageName { get { return _nom.Length >= 2 ? _nom.Substring(0, 2) : "iv"; } }
        public virtual object GetFormat(Type formatType) { return null; }
        public override string ToString() { return _nom; }
    }
}
