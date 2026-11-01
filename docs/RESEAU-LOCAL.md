# Réseau local partagé entre applis Windows et Linux

Winlux n'a qu'un noyau : une appli Windows et une appli Linux utilisent la
même pile réseau, les mêmes ports et la même boucle locale. Un jeu ou un
outil Windows qui ouvre un serveur sur `localhost:8080` s'ouvre dans Firefox,
et une appli Windows joint un serveur Linux local (base de données, serveur
web de développement…) comme sous Windows.

## WinSock : ce qui est traduit

- `AF_INET6` vaut 23 sous Windows et 10 sous Linux : `socket`, `WSASocket`,
  `bind`, `connect`, `sendto`, `accept`, `recvfrom`, `getsockname`,
  `getpeername`, `getaddrinfo` (indices et résultats), `getnameinfo`,
  `WSAStringToAddress` et `WSAAddressToString` convertissent la famille.
  Sans cela, un serveur double pile (`[::]`) ne démarrait pas.
- Options `IPPROTO_IP` et `IPPROTO_IPV6` (numéros différents de Linux) :
  TTL, multidiffusion (jeux en réseau local, découverte d'appareils),
  `IPV6_V6ONLY`… Une option inconnue est refusée au lieu d'être passée telle
  quelle au noyau.
- `SO_EXCLUSIVEADDRUSE` est accepté : un port lié sous Linux est déjà
  exclusif.

## Voir qui écoute

- iphlpapi : `GetExtendedTcpTable`, `GetExtendedUdpTable` (IPv4 et IPv6,
  avec le PID), `GetTcpTable`, `GetTcpTable2`, `GetUdpTable`, lus dans
  `/proc/net`. netstat, TCPView et les jeux qui cherchent un port libre
  voient aussi les programmes Linux.
- Pare-feu Winlux, page **Ports ouverts** : chaque port en écoute avec le nom
  de l'appli (description du `.exe`, sinon le nom du programme Linux), le
  protocole, et s'il est joignable depuis le réseau ou depuis cet ordinateur
  uniquement. Écrit par le service dans `/run/winlux/firewall/ports.txt` ;
  `winlux --firewall ports` affiche la même liste.

## Proxy commun

Source unique : `/etc/winlux/proxy.conf` ; à défaut, le proxy Linux
(`http_proxy`… de l'environnement, puis `/etc/environment` écrit par
l'installateur).

- WinHTTP (accès par défaut ou automatique) passe par ce proxy, sauf pour
  les exceptions et la boucle locale ;
  `WinHttpGetDefaultProxyConfiguration` et
  `WinHttpGetIEProxyConfigForCurrentUser` le renvoient.
- Registre : `ProxyEnable`, `ProxyServer`, `ProxyOverride` de
  `HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings`
  reflètent ce proxy.
- Une appli Windows qui change ces valeurs (VPN, réglages d'un navigateur)
  met à jour `proxy.conf` et `/etc/winlux/proxy.env`, chargé par la session :
  les applis Linux lancées ensuite utilisent le même proxy.

Code : `runtime/src/netports.rs`, `runtime/src/proxy.rs`,
`runtime/src/dlls/ws2_32.rs`, `runtime/src/registry/mod.rs` (proxy).
