// System.ComponentModel.Component et Container (#2305), d'après MS Learn :
// « Component Class », « Component.Dispose Method », « Container Class »,
// « IComponent Interface », « ISite Interface », « IContainer Interface »,
// « EventHandlerList Class », « MarshalByRefObject Class ».
// Corsair.Service.exe (iCUE) mourait sur Component::Dispose.
using System;

namespace System
{
    public interface IServiceProvider
    {
        object GetService(Type serviceType);
    }

    // Pas de communication à distance dans Winlux : la classe ne sert que
    // de base aux types qui en héritent sous .NET Framework.
    public abstract class MarshalByRefObject
    {
        protected MarshalByRefObject() { }
    }
}

namespace System.ComponentModel
{
    public interface IComponent : IDisposable
    {
        ISite Site { get; set; }
        event EventHandler Disposed;
    }

    public interface IContainer : IDisposable
    {
        ComponentCollection Components { get; }
        void Add(IComponent component);
        void Add(IComponent component, string name);
        void Remove(IComponent component);
    }

    public interface ISite : IServiceProvider
    {
        IComponent Component { get; }
        IContainer Container { get; }
        bool DesignMode { get; }
        string Name { get; set; }
    }

    public class ComponentCollection
    {
        private readonly IComponent[] _composants;

        public ComponentCollection(IComponent[] components)
        {
            _composants = components ?? new IComponent[0];
        }

        public int Count { get { return _composants.Length; } }
        public virtual IComponent this[int index] { get { return _composants[index]; } }

        public virtual IComponent this[string name]
        {
            get
            {
                if (name == null) return null;
                foreach (IComponent c in _composants)
                {
                    if (c != null && c.Site != null && c.Site.Name != null
                        && string.Equals(c.Site.Name, name, StringComparison.OrdinalIgnoreCase))
                        return c;
                }
                return null;
            }
        }

        public void CopyTo(IComponent[] array, int index) { Array.Copy(_composants, 0, array, index, _composants.Length); }
    }

    public sealed class EventHandlerList : IDisposable
    {
        private sealed class Entree
        {
            public object Cle;
            public Delegate Gestionnaire;
            public Entree Suivante;
        }

        private Entree _tete;

        private Entree Trouver(object key)
        {
            for (Entree e = _tete; e != null; e = e.Suivante)
                if (e.Cle == key) return e;
            return null;
        }

        public Delegate this[object key]
        {
            get { Entree e = Trouver(key); return e == null ? null : e.Gestionnaire; }
            set
            {
                Entree e = Trouver(key);
                if (e != null) e.Gestionnaire = value;
                else _tete = new Entree { Cle = key, Gestionnaire = value, Suivante = _tete };
            }
        }

        public void AddHandler(object key, Delegate value)
        {
            Entree e = Trouver(key);
            if (e != null) e.Gestionnaire = Delegate.Combine(e.Gestionnaire, value);
            else _tete = new Entree { Cle = key, Gestionnaire = value, Suivante = _tete };
        }

        public void RemoveHandler(object key, Delegate value)
        {
            Entree e = Trouver(key);
            if (e != null) e.Gestionnaire = Delegate.Remove(e.Gestionnaire, value);
        }

        public void AddHandlers(EventHandlerList listToAddFrom)
        {
            if (listToAddFrom == null) throw new ArgumentNullException("listToAddFrom");
            for (Entree e = listToAddFrom._tete; e != null; e = e.Suivante)
                AddHandler(e.Cle, e.Gestionnaire);
        }

        public void Dispose() { _tete = null; }
    }

    public class Component : MarshalByRefObject, IComponent, IDisposable
    {
        private static readonly object s_disposed = new object();
        private ISite _site;
        private EventHandlerList _evenements;

        public Component() { }

        ~Component() { Dispose(false); }

        protected virtual bool CanRaiseEvents { get { return true; } }

        protected EventHandlerList Events
        {
            get
            {
                if (_evenements == null) _evenements = new EventHandlerList();
                return _evenements;
            }
        }

        public virtual ISite Site { get { return _site; } set { _site = value; } }
        public IContainer Container { get { ISite s = _site; return s == null ? null : s.Container; } }
        protected bool DesignMode { get { ISite s = _site; return s != null && s.DesignMode; } }

        public event EventHandler Disposed
        {
            add { Events.AddHandler(s_disposed, value); }
            remove { Events.RemoveHandler(s_disposed, value); }
        }

        public void Dispose()
        {
            Dispose(true);
            // GC.SuppressFinalize n'est pas encore dans notre mscorlib (#2077) ;
            // Dispose(false) reste sans effet si un finaliseur passe ensuite.
        }

        protected virtual void Dispose(bool disposing)
        {
            if (!disposing) return;
            lock (this)
            {
                ISite s = _site;
                if (s != null && s.Container != null) s.Container.Remove(this);
                if (_evenements != null)
                {
                    EventHandler h = (EventHandler)_evenements[s_disposed];
                    if (h != null) h(this, EventArgs.Empty);
                }
            }
        }

        protected virtual object GetService(Type service)
        {
            ISite s = _site;
            return s == null ? null : s.GetService(service);
        }

        public override string ToString()
        {
            ISite s = _site;
            if (s != null) return s.Name + " [" + GetType().FullName + "]";
            return GetType().FullName;
        }
    }

    public class Container : IContainer, IDisposable
    {
        private sealed class Site : ISite
        {
            private readonly IComponent _composant;
            private readonly Container _conteneur;
            private string _nom;

            internal Site(IComponent component, Container container, string name)
            {
                _composant = component;
                _conteneur = container;
                _nom = name;
            }

            public IComponent Component { get { return _composant; } }
            public IContainer Container { get { return _conteneur; } }
            public bool DesignMode { get { return false; } }
            public string Name
            {
                get { return _nom; }
                set
                {
                    if (value == null || _nom == null || !string.Equals(value, _nom, StringComparison.OrdinalIgnoreCase))
                        _conteneur.ValidateName(_composant, value);
                    _nom = value;
                }
            }

            public object GetService(Type service)
            {
                return service == typeof(ISite) ? this : _conteneur.GetService(service);
            }
        }

        private readonly System.Collections.Generic.List<ISite> _sites = new System.Collections.Generic.List<ISite>();

        public Container() { }

        ~Container() { Dispose(false); }

        public virtual ComponentCollection Components
        {
            get
            {
                lock (_sites)
                {
                    IComponent[] t = new IComponent[_sites.Count];
                    for (int i = 0; i < t.Length; i++) t[i] = _sites[i].Component;
                    return new ComponentCollection(t);
                }
            }
        }

        public virtual void Add(IComponent component) { Add(component, null); }

        public virtual void Add(IComponent component, string name)
        {
            if (component == null) return;
            lock (_sites)
            {
                foreach (ISite s in _sites)
                    if (s.Component == component) return;
                ISite ancien = component.Site;
                if (ancien != null) ancien.Container.Remove(component);
                ValidateName(component, name);
                ISite site = CreateSite(component, name);
                _sites.Add(site);
                component.Site = site;
            }
        }

        protected virtual ISite CreateSite(IComponent component, string name)
        {
            return new Site(component, this, name);
        }

        public virtual void Remove(IComponent component) { Retirer(component, false); }

        protected void RemoveWithoutUnsiting(IComponent component) { Retirer(component, true); }

        private void Retirer(IComponent component, bool garderSite)
        {
            if (component == null) return;
            lock (_sites)
            {
                ISite site = component.Site;
                if (site == null || site.Container != this) return;
                if (!garderSite) component.Site = null;
                _sites.Remove(site);
            }
        }

        protected virtual void ValidateName(IComponent component, string name)
        {
            if (component == null) throw new ArgumentNullException("component");
            if (name == null) return;
            foreach (ISite s in _sites)
            {
                if (s.Component != component && s.Name != null
                    && string.Equals(s.Name, name, StringComparison.OrdinalIgnoreCase))
                    throw new ArgumentException("Duplicate component name '" + name + "'. Component names must be unique and case-insensitive.", "name");
            }
        }

        protected virtual object GetService(Type service)
        {
            return service == typeof(IContainer) ? this : null;
        }

        public void Dispose()
        {
            Dispose(true);
            // GC.SuppressFinalize n'est pas encore dans notre mscorlib (#2077) ;
            // Dispose(false) reste sans effet si un finaliseur passe ensuite.
        }

        // Les composants sont libérés du dernier ajouté au premier.
        protected virtual void Dispose(bool disposing)
        {
            if (!disposing) return;
            lock (_sites)
            {
                while (_sites.Count > 0)
                {
                    ISite s = _sites[_sites.Count - 1];
                    s.Component.Site = null;
                    _sites.RemoveAt(_sites.Count - 1);
                    s.Component.Dispose();
                }
            }
        }
    }
}
