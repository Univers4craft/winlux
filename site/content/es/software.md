---
title: Programas y juegos
description: Los programas y juegos de Windows que se instalan y se ejecutan en Winlux, comprobados uno a uno, y los que siguen en curso.
---

Cada programa de esta página se descargó del sitio oficial de su editor, en su versión para Windows, y después se instaló y se ejecutó en Winlux en nuestro banco de pruebas. La lista se actualiza tras cada comprobación.

> [!note]
> Lista elaborada el 22 de noviembre de 2026, según la versión publicada v0.2.21 y las comprobaciones hechas desde entonces. Algunas filas corresponden a la próxima versión, la v0.3, aún no publicada.

## Programas que funcionan

| Programa | Categoría | Comprobado |
|---|---|---|
| VLC | Multimedia | Se instala, reproduce vídeos y música |
| Audacity 3.6 | Multimedia | Instalación y desinstalación limpias, se abre y se puede usar |
| ScreenToGif | Multimedia | Se abre y graba la pantalla |
| OBS Studio | Multimedia | Abre su ventana con la vista previa ([#2262](issue:2262), [#2313](issue:2313)) |
| Paint.NET 5.1 | Imagen | Se instala (Winlux descarga el paquete completo del editor), se abre y dibuja; el descargador del instalador web todavía no llega al final ([#2752](issue:2752)) |
| 7-Zip (64 y 32 bits) | Archivos | Se instala, abre y crea archivos comprimidos, en ventana y en consola |
| Notepad++ 8.6 | Documentos | Instalación y desinstalación limpias, edita archivos |
| SumatraPDF 3.6 | Documentos | Abre y muestra archivos PDF |
| Flow Launcher | Herramientas | Se instala y busca programas y archivos |
| PuTTY (64 y 32 bits) | Red | Se ejecuta y abre sesiones SSH |
| FileZilla | Red | Se instala y transfiere archivos por FTP y SFTP |
| WinSCP (32 bits) | Red | Se ejecuta y transfiere archivos por SFTP |
| Claude Code | Desarrollo | Se instala y funciona en el terminal |
| Pythonwin | Desarrollo | Abre su ventana principal ([#1785](issue:1785)) |

## Juegos que funcionan

| Juego | Gráficos | Comprobado |
|---|---|---|
| Warzone 2100 4.7 | Direct3D 11 u OpenGL | Se ejecuta y se puede jugar |
| SuperTuxKart 1.5 | OpenGL | Se ejecuta y se puede jugar, unas 116 imágenes por segundo en el banco |
| Xonotic | OpenGL | Se ejecuta y se puede jugar, más de 220 imágenes por segundo en el banco |

## Instalados limpiamente, uso aún no comprobado

| Programa | Comprobado |
|---|---|
| WinMerge 2.16 | Instalación y desinstalación limpias |
| ShareX 16.1 | Instalación y desinstalación limpias |

## En curso

| Programa | Estado |
|---|---|
| Office | La herramienta de implementación descarga Office y comprueba sus firmas; la instalación todavía no llega al final ([#2740](issue:2740)) |
| Adobe Acrobat Reader | Se instala; la aplicación todavía no se puede usar ([#236](issue:236)) |
| Teams | Se instala y arranca, sin llegar a la pantalla de inicio de sesión ([#2511](issue:2511), [#2726](issue:2726)) |
| Adobe Creative Cloud | El instalador arranca y contacta con los servidores del editor, su ventana todavía no aparece ([#2784](issue:2784)) |
| OneDrive | El instalador todavía no arranca ([#2851](issue:2851)) |
| Claude para Windows | Se instala y se abre; los fallos al maximizar y mover están corregidos, falta volver a comprobarlo todo |
| Steam | Se instala y descarga su cliente, todavía no se conecta ([#285](issue:285)) |
| IrfanView | Todavía no se ha probado a fondo ([#488](issue:488)) |
| Photoshop | Todavía no se ha probado a fondo |

## ¿Tu programa o juego no está en la lista?

¡Pruébalo! Si no se abre, Winlux te propone notificarlo con el nombre de la función que falta, y eso es lo que nos permite hacerlo funcionar. Consulta [Obtener ayuda](page:guide-help#informar-de-un-problema) y, para el detalle por tipo de programa, [Aplicaciones](page:apps).
