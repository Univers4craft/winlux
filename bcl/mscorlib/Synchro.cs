// Mutex et Semaphore nommés de la mscorlib Winlux (#1988), et les types de
// sécurité qu'on leur passe (MutexSecurity, SemaphoreSecurity,
// SecurityIdentifier…).
// Source : la documentation publique de .NET Framework 4.x (MS Learn) pour
// les signatures, les exceptions et les valeurs des énumérations ; les
// chaînes SID des comptes bien connus viennent de la documentation publique
// « Well-known SIDs ».
// Un Mutex ou un Semaphore est l'objet de kernel32 du même nom : un nom
// déjà pris dans un autre processus du compte désigne le même objet (#20).
// Les listes de contrôle d'accès sont acceptées et conservées, mais pas
// appliquées : Winlux n'a pas d'ACL sur ses objets nommés.
using System.Collections.Generic;
using System.Runtime.CompilerServices;
using System.Security.AccessControl;
using System.Security.Principal;

namespace System
{
    public class ApplicationException : Exception
    {
        public ApplicationException() : base("Error in the application.") { }
        public ApplicationException(string message) : base(message) { }
        public ApplicationException(string message, Exception innerException) : base(message, innerException) { }
    }

    public class UnauthorizedAccessException : SystemException
    {
        public UnauthorizedAccessException() : base("Attempted to perform an unauthorized operation.") { }
        public UnauthorizedAccessException(string message) : base(message) { }
        public UnauthorizedAccessException(string message, Exception inner) : base(message, inner) { }
    }
}

namespace System.Threading
{
    public class AbandonedMutexException : SystemException
    {
        public AbandonedMutexException() : base("The wait completed due to an abandoned mutex.") { }
        public AbandonedMutexException(string message) : base(message) { }
    }

    public class WaitHandleCannotBeOpenedException : ApplicationException
    {
        public WaitHandleCannotBeOpenedException() : base("No handle of the given name exists.") { }
        public WaitHandleCannotBeOpenedException(string message) : base(message) { }
        public WaitHandleCannotBeOpenedException(string message, Exception innerException) : base(message, innerException) { }
    }

    internal static class Noms
    {
        internal const int MaxPath = 260;

        internal static void Verifier(string name)
        {
            if (name != null && name.Length > MaxPath)
                throw new ArgumentException("The name can be no more than 260 characters in length.", "name");
        }

        internal static void VerifierOuverture(string name)
        {
            if (name == null) throw new ArgumentNullException("name");
            if (name.Length == 0) throw new ArgumentException("Empty name is not legal.", "name");
            Verifier(name);
        }

        // Erreur rendue par le moteur (opposé du code d'erreur Win32).
        internal static Exception Erreur(long r, string name)
        {
            int code = (int)(-r);
            if (code == 5) return new UnauthorizedAccessException("Access to the path '" + name + "' is denied.");
            if (code == 6) return new WaitHandleCannotBeOpenedException("A WaitHandle with system-wide name '" + name + "' cannot be created. A WaitHandle of a different type might have the same name.");
            if (code == 2 || code == 3) return new WaitHandleCannotBeOpenedException("No handle of the given name exists.");
            return new InvalidOperationException("Win32 error " + code + ".");
        }
    }

    public sealed class Mutex : WaitHandle
    {
        [MethodImpl(MethodImplOptions.InternalCall)]
        private static extern long Creer(bool initiallyOwned, string name);
        [MethodImpl(MethodImplOptions.InternalCall)]
        private static extern long Ouvrir(string name);
        [MethodImpl(MethodImplOptions.InternalCall)]
        private static extern bool Liberer(long handle);

        private MutexSecurity _securite;

        private Mutex(long handle) { _noyau = handle; }

        public Mutex() : this(false, null) { }
        public Mutex(bool initiallyOwned) : this(initiallyOwned, null) { }

        public Mutex(bool initiallyOwned, string name)
        {
            bool createdNew;
            Init(initiallyOwned, name, out createdNew);
        }

        public Mutex(bool initiallyOwned, string name, out bool createdNew)
        {
            Init(initiallyOwned, name, out createdNew);
        }

        public Mutex(bool initiallyOwned, string name, out bool createdNew, MutexSecurity mutexSecurity)
        {
            Init(initiallyOwned, name, out createdNew);
            _securite = mutexSecurity;
        }

        private void Init(bool initiallyOwned, string name, out bool createdNew)
        {
            Noms.Verifier(name);
            long r = Creer(initiallyOwned, name);
            if (r < 0) throw Noms.Erreur(r, name);
            _noyau = r >> 1;
            createdNew = (r & 1) != 0;
        }

        public static Mutex OpenExisting(string name) { return OpenExisting(name, MutexRights.Modify | MutexRights.Synchronize); }

        public static Mutex OpenExisting(string name, MutexRights rights)
        {
            Noms.VerifierOuverture(name);
            long r = Ouvrir(name);
            if (r < 0) throw Noms.Erreur(r, name);
            return new Mutex(r);
        }

        public static bool TryOpenExisting(string name, out Mutex result)
        {
            return TryOpenExisting(name, MutexRights.Modify | MutexRights.Synchronize, out result);
        }

        public static bool TryOpenExisting(string name, MutexRights rights, out Mutex result)
        {
            Noms.VerifierOuverture(name);
            long r = Ouvrir(name);
            if (r == -2 || r == -3) { result = null; return false; }
            if (r < 0) throw Noms.Erreur(r, name);
            result = new Mutex(r);
            return true;
        }

        public void ReleaseMutex()
        {
            if (_noyau == 0) throw new ObjectDisposedException(null, "Safe handle has been closed");
            if (!Liberer(_noyau))
                throw new ApplicationException("Object synchronization method was called from an unsynchronized block of code.");
        }

        public MutexSecurity GetAccessControl() { return _securite ?? new MutexSecurity(); }
        public void SetAccessControl(MutexSecurity mutexSecurity)
        {
            if (mutexSecurity == null) throw new ArgumentNullException("mutexSecurity");
            _securite = mutexSecurity;
        }
    }

    public sealed class Semaphore : WaitHandle
    {
        [MethodImpl(MethodImplOptions.InternalCall)]
        private static extern long Creer(int initialCount, int maximumCount, string name);
        [MethodImpl(MethodImplOptions.InternalCall)]
        private static extern long Ouvrir(string name);
        [MethodImpl(MethodImplOptions.InternalCall)]
        private static extern long Liberer(long handle, int releaseCount);

        private SemaphoreSecurity _securite;

        private Semaphore(long handle) { _noyau = handle; }

        public Semaphore(int initialCount, int maximumCount) : this(initialCount, maximumCount, null) { }

        public Semaphore(int initialCount, int maximumCount, string name)
        {
            bool createdNew;
            Init(initialCount, maximumCount, name, out createdNew);
        }

        public Semaphore(int initialCount, int maximumCount, string name, out bool createdNew)
        {
            Init(initialCount, maximumCount, name, out createdNew);
        }

        public Semaphore(int initialCount, int maximumCount, string name, out bool createdNew, SemaphoreSecurity semaphoreSecurity)
        {
            Init(initialCount, maximumCount, name, out createdNew);
            _securite = semaphoreSecurity;
        }

        private void Init(int initialCount, int maximumCount, string name, out bool createdNew)
        {
            if (initialCount < 0) throw new ArgumentOutOfRangeException("initialCount", "Non-negative number required.");
            if (maximumCount < 1) throw new ArgumentOutOfRangeException("maximumCount", "Positive number required.");
            if (initialCount > maximumCount) throw new ArgumentException("The initial count for the semaphore must be greater than or equal to zero and less than the maximum count.");
            Noms.Verifier(name);
            long r = Creer(initialCount, maximumCount, name);
            if (r < 0) throw Noms.Erreur(r, name);
            _noyau = r >> 1;
            createdNew = (r & 1) != 0;
        }

        public static Semaphore OpenExisting(string name) { return OpenExisting(name, SemaphoreRights.Modify | SemaphoreRights.Synchronize); }

        public static Semaphore OpenExisting(string name, SemaphoreRights rights)
        {
            Noms.VerifierOuverture(name);
            long r = Ouvrir(name);
            if (r < 0) throw Noms.Erreur(r, name);
            return new Semaphore(r);
        }

        public static bool TryOpenExisting(string name, out Semaphore result)
        {
            return TryOpenExisting(name, SemaphoreRights.Modify | SemaphoreRights.Synchronize, out result);
        }

        public static bool TryOpenExisting(string name, SemaphoreRights rights, out Semaphore result)
        {
            Noms.VerifierOuverture(name);
            long r = Ouvrir(name);
            if (r == -2 || r == -3) { result = null; return false; }
            if (r < 0) throw Noms.Erreur(r, name);
            result = new Semaphore(r);
            return true;
        }

        public int Release() { return Release(1); }

        public int Release(int releaseCount)
        {
            if (releaseCount < 1) throw new ArgumentOutOfRangeException("releaseCount", "Non-negative number required.");
            if (_noyau == 0) throw new ObjectDisposedException(null, "Safe handle has been closed");
            long r = Liberer(_noyau, releaseCount);
            if (r == -298) throw new SemaphoreFullException();
            if (r < 0) throw Noms.Erreur(r, null);
            return (int)r;
        }

        public SemaphoreSecurity GetAccessControl() { return _securite ?? new SemaphoreSecurity(); }
        public void SetAccessControl(SemaphoreSecurity semaphoreSecurity)
        {
            if (semaphoreSecurity == null) throw new ArgumentNullException("semaphoreSecurity");
            _securite = semaphoreSecurity;
        }
    }
}

namespace System.Security.Principal
{
    public enum WellKnownSidType
    {
        NullSid = 0,
        WorldSid = 1,
        LocalSid = 2,
        CreatorOwnerSid = 3,
        CreatorGroupSid = 4,
        CreatorOwnerServerSid = 5,
        CreatorGroupServerSid = 6,
        NTAuthoritySid = 7,
        DialupSid = 8,
        NetworkSid = 9,
        BatchSid = 10,
        InteractiveSid = 11,
        ServiceSid = 12,
        AnonymousSid = 13,
        ProxySid = 14,
        EnterpriseControllersSid = 15,
        SelfSid = 16,
        AuthenticatedUserSid = 17,
        RestrictedCodeSid = 18,
        TerminalServerSid = 19,
        RemoteLogonIdSid = 20,
        LogonIdsSid = 21,
        LocalSystemSid = 22,
        LocalServiceSid = 23,
        NetworkServiceSid = 24,
        BuiltinDomainSid = 25,
        BuiltinAdministratorsSid = 26,
        BuiltinUsersSid = 27,
        BuiltinGuestsSid = 28,
    }

    public abstract class IdentityReference
    {
        internal IdentityReference() { }
        public abstract string Value { get; }
        public abstract bool IsValidTargetType(Type targetType);
        public abstract IdentityReference Translate(Type targetType);
        public override string ToString() { return Value; }
    }

    public sealed class SecurityIdentifier : IdentityReference
    {
        // Chaîne SID et nom de compte des SID bien connus.
        private static readonly string[] s_sid =
        {
            "S-1-0-0", "S-1-1-0", "S-1-2-0", "S-1-3-0", "S-1-3-1", "S-1-3-2", "S-1-3-3",
            "S-1-5", "S-1-5-1", "S-1-5-2", "S-1-5-3", "S-1-5-4", "S-1-5-6", "S-1-5-7",
            "S-1-5-8", "S-1-5-9", "S-1-5-10", "S-1-5-11", "S-1-5-12", "S-1-5-13",
            "S-1-5-14", "S-1-5-5", "S-1-5-18", "S-1-5-19", "S-1-5-20", "S-1-5-32",
            "S-1-5-32-544", "S-1-5-32-545", "S-1-5-32-546",
        };

        private static readonly string[] s_compte =
        {
            "NULL SID", "Everyone", "LOCAL", "CREATOR OWNER", "CREATOR GROUP", "CREATOR OWNER SERVER", "CREATOR GROUP SERVER",
            null, "NT AUTHORITY\\DIALUP", "NT AUTHORITY\\NETWORK", "NT AUTHORITY\\BATCH", "NT AUTHORITY\\INTERACTIVE", "NT AUTHORITY\\SERVICE", "NT AUTHORITY\\ANONYMOUS LOGON",
            "NT AUTHORITY\\PROXY", "NT AUTHORITY\\ENTERPRISE DOMAIN CONTROLLERS", "NT AUTHORITY\\SELF", "NT AUTHORITY\\Authenticated Users", "NT AUTHORITY\\RESTRICTED", "NT AUTHORITY\\TERMINAL SERVER USER",
            "NT AUTHORITY\\REMOTE INTERACTIVE LOGON", null, "NT AUTHORITY\\SYSTEM", "NT AUTHORITY\\LOCAL SERVICE", "NT AUTHORITY\\NETWORK SERVICE", "BUILTIN\\BUILTIN",
            "BUILTIN\\Administrators", "BUILTIN\\Users", "BUILTIN\\Guests",
        };

        private readonly string _valeur;

        public SecurityIdentifier(string sddlForm)
        {
            if (sddlForm == null) throw new ArgumentNullException("sddlForm");
            string s = sddlForm.ToUpperInvariant();
            if (s == "WD") s = "S-1-1-0";
            else if (s == "SY") s = "S-1-5-18";
            else if (s == "BA") s = "S-1-5-32-544";
            else if (s == "BU") s = "S-1-5-32-545";
            else if (s == "AU") s = "S-1-5-11";
            else if (!s.StartsWith("S-1-")) throw new ArgumentException("Value was invalid.", "sddlForm");
            _valeur = s;
        }

        public SecurityIdentifier(WellKnownSidType sidType, SecurityIdentifier domainSid)
        {
            int i = (int)sidType;
            if (i < 0 || i >= s_sid.Length) throw new ArgumentException("The specified SID type is not supported.", "sidType");
            // LogonIdsSid se termine par l'identifiant de la session : celle
            // du compte connecté est la première.
            _valeur = sidType == WellKnownSidType.LogonIdsSid ? "S-1-5-5-0-0" : s_sid[i];
        }

        public override string Value { get { return _valeur; } }

        public bool IsWellKnown(WellKnownSidType type)
        {
            int i = (int)type;
            return i >= 0 && i < s_sid.Length && s_sid[i] == _valeur;
        }

        public bool IsAccountSid() { return _valeur.StartsWith("S-1-5-21-"); }

        public override bool IsValidTargetType(Type targetType)
        {
            return targetType == typeof(SecurityIdentifier) || targetType == typeof(NTAccount);
        }

        public override IdentityReference Translate(Type targetType)
        {
            if (targetType == null) throw new ArgumentNullException("targetType");
            if (targetType == typeof(SecurityIdentifier)) return this;
            if (targetType != typeof(NTAccount)) throw new ArgumentException("Only NTAccount and SecurityIdentifier are valid target types.", "targetType");
            for (int i = 0; i < s_sid.Length; i++)
                if (s_sid[i] == _valeur && s_compte[i] != null) return new NTAccount(s_compte[i]);
            throw new IdentityNotMappedException();
        }

        public override bool Equals(object o)
        {
            SecurityIdentifier autre = o as SecurityIdentifier;
            return autre != null && autre._valeur == _valeur;
        }

        public override int GetHashCode() { return _valeur.GetHashCode(); }

        public static bool operator ==(SecurityIdentifier a, SecurityIdentifier b)
        {
            if ((object)a == null) return (object)b == null;
            return a.Equals(b);
        }

        public static bool operator !=(SecurityIdentifier a, SecurityIdentifier b) { return !(a == b); }
    }

    public sealed class NTAccount : IdentityReference
    {
        private readonly string _nom;

        public NTAccount(string name)
        {
            if (name == null) throw new ArgumentNullException("name");
            if (name.Length == 0) throw new ArgumentException("Empty name is not legal.", "name");
            _nom = name;
        }

        public NTAccount(string domainName, string accountName)
            : this(string.IsNullOrEmpty(domainName) ? accountName : domainName + "\\" + accountName) { }

        public override string Value { get { return _nom; } }

        public override bool IsValidTargetType(Type targetType)
        {
            return targetType == typeof(SecurityIdentifier) || targetType == typeof(NTAccount);
        }

        public override IdentityReference Translate(Type targetType)
        {
            if (targetType == null) throw new ArgumentNullException("targetType");
            if (targetType == typeof(NTAccount)) return this;
            throw new IdentityNotMappedException();
        }

        public override bool Equals(object o)
        {
            NTAccount autre = o as NTAccount;
            return autre != null && string.Equals(autre._nom, _nom, StringComparison.OrdinalIgnoreCase);
        }

        public override int GetHashCode() { return _nom.ToUpperInvariant().GetHashCode(); }
    }

    public sealed class IdentityNotMappedException : SystemException
    {
        public IdentityNotMappedException() : base("Some or all identity references could not be translated.") { }
        public IdentityNotMappedException(string message) : base(message) { }
    }
}

namespace System.Security.AccessControl
{
    public enum AccessControlType
    {
        Allow = 0,
        Deny = 1,
    }

    [Flags]
    public enum InheritanceFlags
    {
        None = 0,
        ContainerInherit = 1,
        ObjectInherit = 2,
    }

    [Flags]
    public enum PropagationFlags
    {
        None = 0,
        NoPropagateInherit = 1,
        InheritOnly = 2,
    }

    [Flags]
    public enum MutexRights
    {
        Modify = 0x000001,
        Delete = 0x010000,
        ReadPermissions = 0x020000,
        ChangePermissions = 0x040000,
        TakeOwnership = 0x080000,
        Synchronize = 0x100000,
        FullControl = 0x1F0001,
    }

    [Flags]
    public enum SemaphoreRights
    {
        Modify = 0x000002,
        Delete = 0x010000,
        ReadPermissions = 0x020000,
        ChangePermissions = 0x040000,
        TakeOwnership = 0x080000,
        Synchronize = 0x100000,
        FullControl = 0x1F0003,
    }

    public abstract class AuthorizationRule
    {
        private readonly IdentityReference _identite;
        private readonly int _masque;

        protected internal AuthorizationRule(IdentityReference identity, int accessMask, bool isInherited,
            InheritanceFlags inheritanceFlags, PropagationFlags propagationFlags)
        {
            if (identity == null) throw new ArgumentNullException("identity");
            if (accessMask == 0) throw new ArgumentException("Argument cannot be zero.", "accessMask");
            _identite = identity;
            _masque = accessMask;
            IsInherited = isInherited;
            InheritanceFlags = inheritanceFlags;
            PropagationFlags = propagationFlags;
        }

        public IdentityReference IdentityReference { get { return _identite; } }
        public bool IsInherited { get; private set; }
        public InheritanceFlags InheritanceFlags { get; private set; }
        public PropagationFlags PropagationFlags { get; private set; }
        protected internal int AccessMask { get { return _masque; } }
    }

    public abstract class AccessRule : AuthorizationRule
    {
        protected AccessRule(IdentityReference identity, int accessMask, bool isInherited,
            InheritanceFlags inheritanceFlags, PropagationFlags propagationFlags, AccessControlType type)
            : base(identity, accessMask, isInherited, inheritanceFlags, propagationFlags)
        {
            if (type != AccessControlType.Allow && type != AccessControlType.Deny) throw new ArgumentOutOfRangeException("type");
            AccessControlType = type;
        }

        public AccessControlType AccessControlType { get; private set; }
    }

    public sealed class MutexAccessRule : AccessRule
    {
        public MutexAccessRule(IdentityReference identity, MutexRights eventRights, AccessControlType type)
            : base(identity, (int)eventRights, false, InheritanceFlags.None, PropagationFlags.None, type) { }

        public MutexAccessRule(string identity, MutexRights eventRights, AccessControlType type)
            : this(new NTAccount(identity), eventRights, type) { }

        public MutexRights MutexRights { get { return (MutexRights)AccessMask; } }
    }

    public sealed class SemaphoreAccessRule : AccessRule
    {
        public SemaphoreAccessRule(IdentityReference identity, SemaphoreRights eventRights, AccessControlType type)
            : base(identity, (int)eventRights, false, InheritanceFlags.None, PropagationFlags.None, type) { }

        public SemaphoreAccessRule(string identity, SemaphoreRights eventRights, AccessControlType type)
            : this(new NTAccount(identity), eventRights, type) { }

        public SemaphoreRights SemaphoreRights { get { return (SemaphoreRights)AccessMask; } }
    }

    // Règles conservées et rendues telles quelles : Winlux n'applique pas
    // d'ACL à ses objets nommés.
    internal static class Regles
    {
        internal static void Ajouter<T>(List<T> regles, T rule) where T : AccessRule
        {
            if (rule == null) throw new ArgumentNullException("rule");
            regles.Add(rule);
        }

        internal static void Remplacer<T>(List<T> regles, T rule) where T : AccessRule
        {
            if (rule == null) throw new ArgumentNullException("rule");
            for (int i = regles.Count - 1; i >= 0; i--)
                if (regles[i].IdentityReference.Equals(rule.IdentityReference) && regles[i].AccessControlType == rule.AccessControlType)
                    regles.RemoveAt(i);
            regles.Add(rule);
        }

        internal static bool Retirer<T>(List<T> regles, T rule) where T : AccessRule
        {
            if (rule == null) throw new ArgumentNullException("rule");
            return regles.Remove(rule);
        }
    }

    public sealed class MutexSecurity
    {
        private readonly List<MutexAccessRule> _regles = new List<MutexAccessRule>();

        public MutexSecurity() { }

        public void AddAccessRule(MutexAccessRule rule) { Regles.Ajouter(_regles, rule); }
        public void SetAccessRule(MutexAccessRule rule) { Regles.Remplacer(_regles, rule); }
        public void ResetAccessRule(MutexAccessRule rule) { Regles.Remplacer(_regles, rule); }
        public bool RemoveAccessRule(MutexAccessRule rule) { return Regles.Retirer(_regles, rule); }
        public void RemoveAccessRuleSpecific(MutexAccessRule rule) { Regles.Retirer(_regles, rule); }
        public Type AccessRightType { get { return typeof(MutexRights); } }
        public Type AccessRuleType { get { return typeof(MutexAccessRule); } }
    }

    public sealed class SemaphoreSecurity
    {
        private readonly List<SemaphoreAccessRule> _regles = new List<SemaphoreAccessRule>();

        public SemaphoreSecurity() { }

        public void AddAccessRule(SemaphoreAccessRule rule) { Regles.Ajouter(_regles, rule); }
        public void SetAccessRule(SemaphoreAccessRule rule) { Regles.Remplacer(_regles, rule); }
        public void ResetAccessRule(SemaphoreAccessRule rule) { Regles.Remplacer(_regles, rule); }
        public bool RemoveAccessRule(SemaphoreAccessRule rule) { return Regles.Retirer(_regles, rule); }
        public void RemoveAccessRuleSpecific(SemaphoreAccessRule rule) { Regles.Retirer(_regles, rule); }
        public Type AccessRightType { get { return typeof(SemaphoreRights); } }
        public Type AccessRuleType { get { return typeof(SemaphoreAccessRule); } }
    }
}
