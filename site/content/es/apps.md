---
title: Aplicaciones probadas
description: Lo que ya funciona en Winlux, lo que todavía no funciona y cómo ayudarnos a ampliar la lista.
---

Winlux abre los programas de Windows gracias a una capa de compatibilidad escrita desde cero. Muchos programas ya funcionan, pero no todos: esta página lo dice con franqueza.

## Por tipo de programa

| Tipo de programa | Estado actual |
|---|---|
| Programa sencillo de 64 bits (herramientas, utilidades, programas de línea de comandos) | Suele funcionar |
| Instalador `.msi`, NSIS o Inno Setup | Funciona |
| Aplicación .NET que incluye su propio entorno de ejecución | Arranca, pero su interfaz no siempre se abre |
| Interfaces WinForms y WPF | Parcial: ScreenToGif y Flow Launcher (WPF) funcionan, muchas otras todavía no |
| Juegos Direct3D 9, 11 y 12 | Parcial, a comprobar juego por juego |
| Programa de 32 bits | Parcial: PuTTY, 7-Zip y WinSCP en 32 bits funcionan, muchos otros todavía no se abren |
| Controladores, antivirus, antitrampas a nivel de sistema | No compatible |
| Aplicaciones en paquete MSIX o AppX | Instalación, accesos directos y desinstalación |

## Programas probados

La lista de programas y juegos comprobados uno a uno, con lo que funciona y lo que sigue en curso, está en la página [Programas y juegos](page:software).

El detalle función por función y el orden de los próximos trabajos están en la wiki: [Compatibilidad de las aplicaciones](wiki:Compatibilite-des-applications).

## Lo que viene

Por orden de impacto en el número de programas desbloqueados: las funciones de Windows más solicitadas, OpenGL y Vulkan para las aplicaciones, las bibliotecas redistribuibles de Visual C++, las conexiones seguras HTTPS, los shaders de Direct3D y luego los programas de 32 bits. La [hoja de ruta](wiki:Feuille-de-route) sigue el avance.

## ¿Su programa no está en la lista?

¡Pruébelo! Si no se abre, Winlux le propone informar de ello, con el nombre de la función que falta: es exactamente lo que nos permite hacerlo funcionar. Vea [Obtener ayuda](page:guide-help#informar-de-un-problema).
