---
title: Actualizar
description: Las nuevas versiones de Winlux se instalan desde Configuración, con un punto de restauración y vuelta atrás automática.
---

## Desde Configuración

1. Abra **Configuración › Actualizaciones › Actualización del SO**.
2. Pulse **«Buscar actualizaciones»**. Si existe una versión más reciente, la página muestra «Actualización disponible», su tamaño y sus novedades.
3. Pulse **«Descargar e instalar»** y siga el progreso.
4. Reinicie cuando le venga bien: Winlux nunca fuerza el reinicio.

![La página Actualización del SO](img:mises-a-jour.jpg "Configuración › Actualizaciones › Actualización del SO: búsqueda, pausa, instalación automática, historial y vuelta a la versión anterior.")

En la misma página, **«Instalar automáticamente las nuevas versiones»** las instala sin que tenga que intervenir, fuera de sus horas activas y nunca durante un juego; **«Pausar actualizaciones»** las pone en pausa. Los paquetes del sistema base se actualizan desde la fila **«Software y controladores de Linux»** de la misma página.

## Lo que protege su sistema

- Cada actualización se descarga de las versiones publicadas del proyecto, y la firma de su descripción, su tamaño y su suma SHA-256 se comprueban antes de instalarla.
- Justo antes se crea un **punto de restauración**; se conservan los tres últimos.
- Si el escritorio deja de arrancar bien tras una actualización, Winlux **vuelve por sí solo** a la versión anterior.
- Sus documentos, sus programas instalados y sus ajustes se conservan.

> [!note]
> **¿Viene de una versión antigua?** Una instalación en v0.2.12 o anterior debe reinstalarse una vez con una imagen reciente: su herramienta de actualización no puede corregirse a sí misma. A partir de la v0.2.13, todo se hace desde Configuración.

## Volver atrás

Si algo va mal tras una actualización, reinicie y elija **«Winlux secours»** (rescate) en el menú de arranque (tecla `s`), y luego «Restaurar el punto de restauración anterior».

Para saber más: [Publicación y actualizaciones](wiki:Publication-et-mises-a-jour).
