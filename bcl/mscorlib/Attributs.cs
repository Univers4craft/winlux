// Attributs d'assembly et attributs courants de la mscorlib Winlux (#1992).
// Source : la documentation publique de .NET Framework 4.x (MS Learn) pour
// les signatures, les propriétés et les valeurs des énumérations.
// Le compilateur les pose sur presque chaque assembly (AssemblyInfo.cs) ;
// un programme les relit par réflexion (version de fichier, produit…).

namespace System
{
    [AttributeUsage(AttributeTargets.All, Inherited = false)]
    public sealed class ObsoleteAttribute : Attribute
    {
        private readonly string _message;
        private readonly bool _error;

        public ObsoleteAttribute() { }
        public ObsoleteAttribute(string message) { _message = message; }
        public ObsoleteAttribute(string message, bool error) { _message = message; _error = error; }

        public string Message { get { return _message; } }
        public bool IsError { get { return _error; } }
    }

    [AttributeUsage(AttributeTargets.All, Inherited = true)]
    public sealed class CLSCompliantAttribute : Attribute
    {
        private readonly bool _conforme;
        public CLSCompliantAttribute(bool isCompliant) { _conforme = isCompliant; }
        public bool IsCompliant { get { return _conforme; } }
    }

    [AttributeUsage(AttributeTargets.Class | AttributeTargets.Struct | AttributeTargets.Enum | AttributeTargets.Delegate, Inherited = false)]
    public sealed class SerializableAttribute : Attribute { }

    [AttributeUsage(AttributeTargets.Field, Inherited = false)]
    public sealed class NonSerializedAttribute : Attribute { }
}

namespace System.Reflection
{
    [AttributeUsage(AttributeTargets.Assembly, Inherited = false)]
    public sealed class AssemblyTitleAttribute : Attribute
    {
        private readonly string _v;
        public AssemblyTitleAttribute(string title) { _v = title; }
        public string Title { get { return _v; } }
    }

    [AttributeUsage(AttributeTargets.Assembly, Inherited = false)]
    public sealed class AssemblyDescriptionAttribute : Attribute
    {
        private readonly string _v;
        public AssemblyDescriptionAttribute(string description) { _v = description; }
        public string Description { get { return _v; } }
    }

    [AttributeUsage(AttributeTargets.Assembly, Inherited = false)]
    public sealed class AssemblyConfigurationAttribute : Attribute
    {
        private readonly string _v;
        public AssemblyConfigurationAttribute(string configuration) { _v = configuration; }
        public string Configuration { get { return _v; } }
    }

    [AttributeUsage(AttributeTargets.Assembly, Inherited = false)]
    public sealed class AssemblyCompanyAttribute : Attribute
    {
        private readonly string _v;
        public AssemblyCompanyAttribute(string company) { _v = company; }
        public string Company { get { return _v; } }
    }

    [AttributeUsage(AttributeTargets.Assembly, Inherited = false)]
    public sealed class AssemblyProductAttribute : Attribute
    {
        private readonly string _v;
        public AssemblyProductAttribute(string product) { _v = product; }
        public string Product { get { return _v; } }
    }

    [AttributeUsage(AttributeTargets.Assembly, Inherited = false)]
    public sealed class AssemblyCopyrightAttribute : Attribute
    {
        private readonly string _v;
        public AssemblyCopyrightAttribute(string copyright) { _v = copyright; }
        public string Copyright { get { return _v; } }
    }

    [AttributeUsage(AttributeTargets.Assembly, Inherited = false)]
    public sealed class AssemblyTrademarkAttribute : Attribute
    {
        private readonly string _v;
        public AssemblyTrademarkAttribute(string trademark) { _v = trademark; }
        public string Trademark { get { return _v; } }
    }

    [AttributeUsage(AttributeTargets.Assembly, Inherited = false)]
    public sealed class AssemblyCultureAttribute : Attribute
    {
        private readonly string _v;
        public AssemblyCultureAttribute(string culture) { _v = culture; }
        public string Culture { get { return _v; } }
    }

    [AttributeUsage(AttributeTargets.Assembly, Inherited = false)]
    public sealed class AssemblyFileVersionAttribute : Attribute
    {
        private readonly string _v;
        public AssemblyFileVersionAttribute(string version)
        {
            if (version == null) throw new ArgumentNullException("version");
            _v = version;
        }
        public string Version { get { return _v; } }
    }

    [AttributeUsage(AttributeTargets.Assembly, Inherited = false)]
    public sealed class AssemblyInformationalVersionAttribute : Attribute
    {
        private readonly string _v;
        public AssemblyInformationalVersionAttribute(string informationalVersion) { _v = informationalVersion; }
        public string InformationalVersion { get { return _v; } }
    }

    [AttributeUsage(AttributeTargets.Assembly, Inherited = false)]
    public sealed class AssemblyKeyFileAttribute : Attribute
    {
        private readonly string _v;
        public AssemblyKeyFileAttribute(string keyFile) { _v = keyFile; }
        public string KeyFile { get { return _v; } }
    }

    [AttributeUsage(AttributeTargets.Assembly, Inherited = false)]
    public sealed class AssemblyDelaySignAttribute : Attribute
    {
        private readonly bool _v;
        public AssemblyDelaySignAttribute(bool delaySign) { _v = delaySign; }
        public bool DelaySign { get { return _v; } }
    }
}

namespace System.Diagnostics
{
    [AttributeUsage(AttributeTargets.Assembly | AttributeTargets.Module, AllowMultiple = false)]
    public sealed class DebuggableAttribute : Attribute
    {
        [Flags]
        public enum DebuggingModes
        {
            None = 0,
            Default = 1,
            IgnoreSymbolStoreSequencePoints = 2,
            EnableEditAndContinue = 4,
            DisableOptimizations = 256,
        }

        private readonly DebuggingModes _modes;

        public DebuggableAttribute(bool isJITTrackingEnabled, bool isJITOptimizerDisabled)
        {
            _modes = (isJITTrackingEnabled ? DebuggingModes.Default : 0)
                | (isJITOptimizerDisabled ? DebuggingModes.DisableOptimizations : 0);
        }

        public DebuggableAttribute(DebuggingModes modes) { _modes = modes; }

        public bool IsJITTrackingEnabled { get { return (_modes & DebuggingModes.Default) != 0; } }
        public bool IsJITOptimizerDisabled { get { return (_modes & DebuggingModes.DisableOptimizations) != 0; } }
        public DebuggingModes DebuggingFlags { get { return _modes; } }
    }

    [AttributeUsage(AttributeTargets.Class | AttributeTargets.Method, AllowMultiple = true)]
    public sealed class ConditionalAttribute : Attribute
    {
        private readonly string _v;
        public ConditionalAttribute(string conditionString) { _v = conditionString; }
        public string ConditionString { get { return _v; } }
    }
}

namespace System.Runtime.InteropServices
{
    [AttributeUsage(AttributeTargets.Assembly | AttributeTargets.Class | AttributeTargets.Struct | AttributeTargets.Enum | AttributeTargets.Method | AttributeTargets.Property | AttributeTargets.Field | AttributeTargets.Interface | AttributeTargets.Delegate, Inherited = false)]
    public sealed class ComVisibleAttribute : Attribute
    {
        private readonly bool _v;
        public ComVisibleAttribute(bool visibility) { _v = visibility; }
        public bool Value { get { return _v; } }
    }

    [AttributeUsage(AttributeTargets.Assembly | AttributeTargets.Class | AttributeTargets.Struct | AttributeTargets.Enum | AttributeTargets.Interface | AttributeTargets.Delegate, Inherited = false)]
    public sealed class GuidAttribute : Attribute
    {
        private readonly string _v;
        public GuidAttribute(string guid) { _v = guid; }
        public string Value { get { return _v; } }
    }
}

namespace System.Runtime.Versioning
{
    [AttributeUsage(AttributeTargets.Assembly, AllowMultiple = false, Inherited = false)]
    public sealed class TargetFrameworkAttribute : Attribute
    {
        private readonly string _nom;

        public TargetFrameworkAttribute(string frameworkName)
        {
            if (frameworkName == null) throw new ArgumentNullException("frameworkName");
            _nom = frameworkName;
        }

        public string FrameworkName { get { return _nom; } }
        public string FrameworkDisplayName { get; set; }
    }
}

namespace System.Resources
{
    public enum UltimateResourceFallbackLocation
    {
        MainAssembly = 0,
        Satellite = 1,
    }

    [AttributeUsage(AttributeTargets.Assembly, AllowMultiple = false)]
    public sealed class NeutralResourcesLanguageAttribute : Attribute
    {
        private readonly string _culture;
        private readonly UltimateResourceFallbackLocation _lieu;

        public NeutralResourcesLanguageAttribute(string cultureName)
        {
            if (cultureName == null) throw new ArgumentNullException("cultureName");
            _culture = cultureName;
        }

        public NeutralResourcesLanguageAttribute(string cultureName, UltimateResourceFallbackLocation location) : this(cultureName)
        {
            _lieu = location;
        }

        public string CultureName { get { return _culture; } }
        public UltimateResourceFallbackLocation Location { get { return _lieu; } }
    }
}

namespace System.ComponentModel
{
    public enum EditorBrowsableState
    {
        Always = 0,
        Never = 1,
        Advanced = 2,
    }

    [AttributeUsage(AttributeTargets.Class | AttributeTargets.Struct | AttributeTargets.Enum | AttributeTargets.Constructor | AttributeTargets.Method | AttributeTargets.Property | AttributeTargets.Field | AttributeTargets.Event | AttributeTargets.Interface | AttributeTargets.Delegate)]
    public sealed class EditorBrowsableAttribute : Attribute
    {
        private readonly EditorBrowsableState _etat;
        public EditorBrowsableAttribute() : this(EditorBrowsableState.Always) { }
        public EditorBrowsableAttribute(EditorBrowsableState state) { _etat = state; }
        public EditorBrowsableState State { get { return _etat; } }
    }

    [AttributeUsage(AttributeTargets.All)]
    public class DefaultValueAttribute : Attribute
    {
        private readonly object _v;
        public DefaultValueAttribute(object value) { _v = value; }
        public DefaultValueAttribute(string value) { _v = value; }
        public DefaultValueAttribute(bool value) { _v = value; }
        public DefaultValueAttribute(int value) { _v = value; }
        public DefaultValueAttribute(long value) { _v = value; }
        public DefaultValueAttribute(double value) { _v = value; }
        public DefaultValueAttribute(float value) { _v = value; }
        public DefaultValueAttribute(char value) { _v = value; }
        public DefaultValueAttribute(byte value) { _v = value; }
        public DefaultValueAttribute(short value) { _v = value; }
        public virtual object Value { get { return _v; } }
    }
}

namespace System.CodeDom.Compiler
{
    [AttributeUsage(AttributeTargets.All, Inherited = false, AllowMultiple = false)]
    public sealed class GeneratedCodeAttribute : Attribute
    {
        private readonly string _outil;
        private readonly string _version;

        public GeneratedCodeAttribute(string tool, string version)
        {
            _outil = tool;
            _version = version;
        }

        public string Tool { get { return _outil; } }
        public string Version { get { return _version; } }
    }
}
