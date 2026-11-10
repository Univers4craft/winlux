---
title: Programas y juegos compatibles
description: La lista de programas y juegos de Windows que se instalan y se abren en Winlux, y de los que todavía están en curso.
---

Cada programa de esta página se instaló y después se abrió en Winlux, en su versión oficial para Windows, tal como se descarga de su editor. Ninguno se modificó para la ocasión.

> [!note]
> Estas pruebas se hacen en el banco de pruebas, en una máquina virtual, con la versión en desarrollo. Algunas correcciones llegarán con la próxima versión publicada.

## Programas que funcionan

Se instalan (o se descomprimen, en las versiones portables), se abren y se usan con normalidad.

| Programa | Para qué sirve | Nota |
|---|---|---|
| VLC | Reproductor de vídeo y audio | |
| Audacity | Grabación y edición de audio | Instalador Inno Setup |
| ScreenToGif | Grabación de la pantalla en animación | Aplicación .NET |
| 7-Zip | Archivos comprimidos y compresión | El administrador de archivos funciona; `7z.exe` en la consola lista bien los archivos, pero indica un error al salir ([#1479](issue:1479)) |
| Notepad++ | Editor de texto | Instalador NSIS, instalación y desinstalación limpias |
| SumatraPDF | Lector de PDF | Versión portable |
| Flow Launcher | Lanzador de programas y búsqueda | Aplicación .NET |
| PuTTY | Terminal SSH | Versiones de 64 y 32 bits |
| FileZilla | Transferencia de archivos por FTP y SFTP | |
| WinSCP | Transferencia de archivos por SFTP | Versión de 32 bits |
| Claude Code | Asistente de programación en línea de comandos | |

## Juegos que funcionan

| Juego | Género | Gráficos |
|---|---|---|
| SuperTuxKart 1.5 | Carreras | OpenGL, en ventana y a pantalla completa |
| Warzone 2100 4.7 | Estrategia en tiempo real | OpenGL, Vulkan y Direct3D 11 |
| Xonotic | Disparos en primera persona | OpenGL, en sus dos versiones |

## Instalación comprobada, uso todavía no

Estos programas se instalan y se desinstalan de forma limpia, pero su uso todavía no se ha comprobado en detalle.

| Programa | Para qué sirve |
|---|---|
| WinMerge | Comparar archivos y carpetas |
| ShareX | Capturas de pantalla y compartir |

## En curso

Estos programas avanzan, pero todavía no se pueden usar de principio a fin. Cada bloqueo tiene su propia incidencia en GitHub.

| Programa | En qué punto está |
|---|---|
| Paint.NET | Se instala y arranca su runtime .NET; su ventana principal todavía no se puede usar ([#138](issue:138)) |
| Cliente de escritorio de Claude para Windows | Arranca, se minimiza y se cierra; maximizarlo o mover su ventana todavía no funciona ([#1476](issue:1476), [#1477](issue:1477)) |
| Steam | Se instala y actualiza su cliente, pero todavía no se puede usar ([#285](issue:285), [#1440](issue:1440)) |
| Photoshop | Todavía no probado hasta el final |
| IrfanView | Todavía no ([#488](issue:488)) |

Lo que todavía no funciona, tipo de programa por tipo de programa, se explica en la página [Aplicaciones probadas](page:apps).

## ¿Su programa no está en la lista?

¡Pruébelo! Si no se abre, Winlux le propone informar de ello, con el nombre de la función que falta. Vea [Obtener ayuda](page:guide-help#informar-de-un-problema).
