---
title: Instalar Winlux
description: El instalador le guía paso a paso, desde la elección del idioma hasta el reinicio, solo en el disco o junto a Windows.
---

Inicie **«Instalar Winlux»** desde el escritorio de la sesión Live, o elija la instalación en el menú de la memoria. El instalador se adapta al tamaño de su pantalla.

![El paso Red del instalador](img:installateur-reseau.jpg "El instalador de Winlux, paso Red: wifi y proxy si hace falta.")

## Los pasos

1. **Idioma y teclado**: francés, inglés, alemán, español y otros; distribuciones AZERTY, QWERTY, QWERTZ…
2. **Red**: conecte el wifi si hace falta; se puede indicar un proxy de empresa.
3. **Tipo de instalación y disco**: vea más abajo.
4. **Importar desde Windows**: recupere el fondo de pantalla, los favoritos y los documentos de una partición de Windows, o migre desde otro PC por la red.
5. **Perfil de uso**: una breve prueba del hardware y luego ofimática, juegos, desarrollo o servidor. Winlux ajusta el procesador, la memoria y la tarjeta gráfica en consecuencia.
6. **Cuenta**: una cuenta **local**, con nombre, contraseña e imagen. No se pide ninguna cuenta en línea.
7. **Resumen**: revíselo todo y pulse «Instalar».

## Elegir dónde instalar

| Opción | Qué ocurre |
|---|---|
| Todo el disco (predeterminado) | Winlux ocupa todo el disco elegido. Su contenido se borra. |
| Junto a Windows | El instalador reduce la partición de Windows dejando un margen; al arrancar, un menú ofrece los dos sistemas. |
| Modo avanzado | Particionado manual: crear, eliminar, redimensionar, formatear. |
| Cifrado | Opcional: la frase de contraseña se pide en cada arranque. |

![La elección del perfil de uso](img:installateur-profil.jpg "El perfil de uso ajusta el sistema a su forma de usarlo; puede cambiarlo más tarde.")

> [!warning]
> Si Windows está instalado en el PC, apáguelo por completo antes de instalar Winlux a su lado (mantenga pulsada Mayús al hacer clic en Apagar): si no, su partición queda bloqueada por el inicio rápido.

Al final, retire la memoria USB y reinicie. El instalador nunca ofrece como destino la memoria desde la que arrancó el PC.

Para saber más: [Instalación en la wiki](wiki:Installation) · [instalación desatendida de varios equipos](repo:docs/AUTOINSTALL.md).
