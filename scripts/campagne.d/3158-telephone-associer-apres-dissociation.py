# Téléphone connecté : « Associer » demandé depuis le PC juste après une
# dissociation venue du téléphone n'est plus perdu (#3158). La demande
# envoyée était effacée par la fin du traitement de la dissociation, après
# que l'état publié la montrait déjà finie ; l'acceptation du téléphone
# devenait alors une nouvelle demande d'appairage. Le contrôle fait arriver
# « Associer » pendant la publication de la dissociation, sans attente.
def _telephone_3158():
    import importlib.util
    from importlib.machinery import SourceFileLoader
    with tempfile.TemporaryDirectory() as tmp:
        env = {"HOME": tmp, "WINLUX_PHONE_CONFIG": f"{tmp}/config", "WINLUX_PHONE_DATA": f"{tmp}/data",
               "WINLUX_PHONE_RUNTIME": f"{tmp}/run", "WINLUX_PHONE_NO_DBUS": "1",
               "WINLUX_PHONE_NO_GUI": "1", "WINLUX_PHONE_NO_FIREWALL": "1", "WINLUX_LANG": "fr"}
        with patch.dict(os.environ, env):
            loader = SourceFileLoader("winlux_phone_3158", str(SCRIPTS / "winlux-phone.py"))
            spec = importlib.util.spec_from_loader("winlux_phone_3158", loader)
            mod = importlib.util.module_from_spec(spec)
            loader.exec_module(mod)

            class Lien:
                id, name, type, cert_der = "tel", "Pixel", "phone", b"der"
                def __init__(self): self.envoyes = []
                def send(self, kind, body): self.envoyes.append((kind, body))

            class Bureau:
                def notify(self, *a, **k): return 0
                def close(self, *a): pass

            d = object.__new__(mod.Daemon)
            d.lock = __import__("threading").RLock()
            lien = Lien()
            d.links, d.pending, d.requested, d.battery = {"tel": lien}, {}, {}, {}
            d.devices = {"tel": {"name": "Pixel", "type": "phone", "prefs": {}}}
            d.desktop, d.own_der, d.public, d.id = Bureau(), b"pc", False, "pc"
            d.on_connected = lambda link: None
            mod.verification_key = lambda *a: "CLE"
            publier = mod.Daemon.publish
            vu = []
            def publie_puis_associe():
                publier(d)
                # Ce que voit le programme à cet instant : téléphone dissocié,
                # données effacées. L'utilisateur clique « Associer ».
                if not vu and "tel" not in d.devices:
                    vu.append(1)
                    d.pair("tel")
            d.publish = publie_puis_associe
            d.handle_pair(lien, {"pair": False})
            demande_gardee = "tel" in d.requested
            d.publish = lambda: publier(d)
            d.handle_pair(lien, {"pair": True})
            associe = "tel" in d.devices and not d.pending
            print(f"#3158 : associer pendant la dissociation : demande gardée={demande_gardee}, "
                  f"acceptation reconnue={associe}, en attente={sorted(d.pending)}")
            return bool(vu) and demande_gardee and associe
ok(_telephone_3158(), "Téléphone connecté : « Associer » juste après une dissociation venue du téléphone n'est pas perdu (#3158)")
