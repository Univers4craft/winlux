---
title: Actualizar
description: Las nuevas versiones de Winlux se instalan desde Configuración, con un punto de restauración y vuelta atrás automática.
---

## Desde Configuración

1. Abra **Configuración › Actualizaciones › Actualización del SO**.
2. Pulse **«Buscar»**. La página indica la versión instalada y la versión disponible.
3. Si existe una versión más reciente, pulse **«Instalar»** y siga el progreso.
4. Reinicie cuando le venga bien: Winlux nunca fuerza el reinicio.

![La página Actualización del SO](img:mises-a-jour.jpg "Configuración › Actualizaciones › Actualización del SO: versión instalada, versión disponible, actualizaciones automáticas.")

En la misma página, **«Actualizaciones automáticas»** instala las nuevas versiones sin que tenga que intervenir. Los paquetes del sistema base se actualizan en **Configuración › Actualizaciones › Actualización de aplicaciones**.

## Lo que protege su sistema

- Cada actualización se descarga de las versiones publicadas del proyecto, y su tamaño y su suma SHA-256 se comprueban antes de instalarla.
- Justo antes se crea un **punto de restauración**; se conservan los tres últimos.
- Si el escritorio deja de arrancar bien tras una actualización, Winlux **vuelve por sí solo** a la versión anterior.
- Sus documentos, sus programas instalados y sus ajustes se conservan.

> [!note]
> **¿Viene de una versión antigua?** Una instalación en v0.2.12 o anterior debe reinstalarse una vez con una imagen reciente: su herramienta de actualización no puede corregirse a sí misma. A partir de la v0.2.13, todo se hace desde Configuración.

## Volver atrás

Si algo va mal tras una actualización, reinicie y elija **«Winlux secours»** (rescate) en el menú de arranque (tecla `s`), y luego «Restaurar el punto de restauración anterior».

Para saber más: [Publicación y actualizaciones](wiki:Publication-et-mises-a-jour).
