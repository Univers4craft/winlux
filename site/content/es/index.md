---
title: Inicio
headline: Los programas de Windows, con un doble clic, en Linux
lead: Winlux es un sistema operativo libre basado en el núcleo Linux. Sus archivos `.exe` se abren con un doble clic, en un escritorio que se parece al que ya conoce, sin máquina virtual oculta y sin capa de compatibilidad prestada.
description: Winlux, el sistema libre con núcleo Linux que abre los programas de Windows con un doble clic. Descarga, guía de instalación y aplicaciones probadas.
---

## Lo que Winlux hace por usted

::: cards
### Basta un doble clic
Un `.exe` descargado se abre como cualquier otro archivo, desde el escritorio o desde el Explorador. Los instaladores `setup.exe` y `.msi` añaden su acceso directo al menú Inicio.

### Un escritorio familiar
Menú Inicio centrado o a la izquierda, barra de tareas, Explorador, Configuración, Administrador de tareas: se orienta desde el primer minuto.

### Sus archivos, en todas partes
Su carpeta personal es la unidad `C:` para los programas de Windows. Documentos, Imágenes y Descargas son los mismos para todas sus aplicaciones.

### Hecho en casa
La compatibilidad con Windows está escrita desde cero, en Rust, para Winlux. En su PC funciona un único sistema: ningún Windows escondido en una máquina virtual.

### Su privacidad
Solo cuentas locales, sin telemetría. Un programa pide su permiso antes de usar la cámara, el micrófono o sus carpetas personales.

### Cuatro idiomas
El sistema, el instalador y este sitio están disponibles en francés, inglés, alemán y español.
:::

## Así se ve

![El escritorio de Winlux con el menú Inicio abierto](img:bureau-menu-demarrer.jpg "El escritorio de Winlux, fondo «Winlux Nuit», menú Inicio abierto.")

::: figure-row
![La ventana de bienvenida en el primer arranque](img:assistant-bienvenue.jpg "En el primer arranque, la ventana de bienvenida lleva al recorrido guiado, al teclado, a la red y a la apariencia.")

![La página Actualizaciones de la Configuración](img:mises-a-jour.jpg "Las actualizaciones se instalan desde Configuración, con vuelta atrás automática.")
:::

## Por dónde empezar

::: steps-grid
### 1. Descargar
Obtenga la imagen de la última versión y, si lo desea, el creador de memoria USB. [Página Descargar](page:download)

### 2. Seguir la guía
Siete pasos, de la memoria USB a su primer programa de Windows, y luego las actualizaciones. [Abrir la guía](page:guide)

### 3. Comprobar sus programas
Lo que ya funciona, lo que todavía no, y cómo comunicárnoslo. [Aplicaciones probadas](page:apps)
:::

## En qué punto está el proyecto

Winlux es una **versión preliminar técnica**: el escritorio, el instalador, las actualizaciones y muchos programas de Windows de 64 bits funcionan, pero todavía no se abren todos los programas. Los programas de 32 bits, los juegos recientes en Direct3D 11 y las aplicaciones de escritorio .NET (WinForms, WPF) forman parte del trabajo en curso. La lista completa se mantiene al día en la [wiki](wiki:Limites-et-problemes-connus).

Este sitio está pensado para quienes descubren Winlux. Para entender cómo está construido, lea la [wiki técnica](wiki:Home) (en francés).
