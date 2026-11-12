// BCL Winlux : System.Nullable<T>, EventArgs, EventHandler et
// ObjectDisposedException (#1952). Licence GPL 3.
// Écrit d'après la documentation publique de l'API (« Nullable<T> Struct »,
// « EventHandler Delegate », « ObjectDisposedException Class ») et
// ECMA-335 (I.8.2.4 et III.4.1 pour l'encadrement d'un Nullable) ; aucun
// code de Microsoft, de Mono ou de Wine.
//
// L'encadrement suit ECMA-335 : box d'un Nullable sans valeur donne null,
// sinon la valeur encadrée ; unbox.any vers Nullable<T> accepte null et
// une valeur T encadrée. Le moteur s'en charge (il.rs), il lit les deux
// champs ci-dessous dans cet ordre : hasValue puis value.

using System.Collections.Generic;

namespace System
{
    public struct Nullable<T> where T : struct
    {
        private readonly bool hasValue;
        internal readonly T value;

        public Nullable(T value)
        {
            this.value = value;
            hasValue = true;
        }

        public bool HasValue { get { return hasValue; } }

        public T Value
        {
            get
            {
                if (!hasValue)
                    throw new InvalidOperationException("Nullable object must have a value.");
                return value;
            }
        }

        public T GetValueOrDefault()
        {
            return value;
        }

        public T GetValueOrDefault(T defaultValue)
        {
            return hasValue ? value : defaultValue;
        }

        public override bool Equals(object other)
        {
            if (!hasValue)
                return other == null;
            if (other == null)
                return false;
            return value.Equals(other);
        }

        public override int GetHashCode()
        {
            return hasValue ? value.GetHashCode() : 0;
        }

        public override string ToString()
        {
            return hasValue ? value.ToString() : "";
        }

        public static implicit operator Nullable<T>(T value)
        {
            return new Nullable<T>(value);
        }

        public static explicit operator T(Nullable<T> value)
        {
            return value.Value;
        }
    }

    public static class Nullable
    {
        public static bool Equals<T>(Nullable<T> n1, Nullable<T> n2) where T : struct
        {
            if (n1.HasValue != n2.HasValue)
                return false;
            if (!n1.HasValue)
                return true;
            return EqualityComparer<T>.Default.Equals(n1.GetValueOrDefault(), n2.GetValueOrDefault());
        }
    }

    public class EventArgs
    {
        public static readonly EventArgs Empty = new EventArgs();

        public EventArgs() { }
    }

    public delegate void EventHandler(object sender, EventArgs e);
    public delegate void EventHandler<TEventArgs>(object sender, TEventArgs e);

    public class ObjectDisposedException : InvalidOperationException
    {
        private readonly string _objectName;

        public ObjectDisposedException(string objectName)
            : this(objectName, "Cannot access a disposed object.") { }

        public ObjectDisposedException(string objectName, string message)
            : base(Compose(objectName, message))
        {
            _objectName = objectName;
        }

        public string ObjectName { get { return (object)_objectName == null ? "" : _objectName; } }

        private static string Compose(string objectName, string message)
        {
            if (string.IsNullOrEmpty(objectName))
                return message;
            return string.Concat(message, "\r\nObject name: '", objectName, "'.");
        }
    }
}
