---
title: Crear la memoria USB
description: Prepare una memoria USB que arranque Winlux en cualquier PC, con el creador de memoria incluido.
---

## Con el creador de memoria USB

El creador de memoria USB de Winlux descarga la última imagen, la comprueba y la escribe en la memoria. Existe para un PC con Windows (`.exe`) y para Linux o Winlux (`.deb`). Obténgalo en la página [Descargar](page:download).

![El creador de memoria USB de Winlux](img:createur-cle-usb.jpg "El creador de memoria USB: elegir la imagen, elegir la memoria, crear.")

1. **Imagen de Winlux**: deje «Descargar la última versión» o elija una imagen ISO ya descargada.
2. **Memoria USB**: elija la memoria en la lista. Solo se ofrecen memorias USB y tarjetas SD, nunca el disco del PC.
3. **Crear la memoria USB**: una ventana le recuerda el nombre y el tamaño de la memoria. No se borra nada antes de que pulse «Borrar y crear la memoria».
4. Espere a que terminen la escritura y luego la verificación: la memoria se vuelve a leer y se compara con la imagen.

> [!warning]
> **Se borra todo el contenido de la memoria.** Copie en otro lugar lo que quiera conservar antes de empezar.

## Con otra herramienta

La imagen de Winlux es «híbrida»: sirve cualquier herramienta que copie una imagen de disco en una memoria USB, y Ventoy también la arranca. En Linux, desde la línea de comandos, sustituyendo `/dev/sdX` por la memoria (y nunca por un disco):

```sh
sudo dd if=winlux-live-v0.2.15.iso of=/dev/sdX bs=4M status=progress conv=fsync
```

## Arrancar el PC desde la memoria

1. Conecte la memoria y encienda (o reinicie) el PC.
2. Pulse enseguida la tecla del menú de arranque: a menudo **F12**, **F11**, **F8** o **Esc**, según el fabricante.
3. Elija la memoria USB. Arranca tanto en UEFI como en BIOS heredada, y Secure Boot puede seguir activado. En una tableta con firmware de arranque de 32 bits (Atom), desactive Secure Boot: la memoria también arranca en ese modo.
4. Aparece el menú de Winlux: elija la sesión Live para probarlo, o la instalación.

> [!tip]
> La sesión Live funciona completamente en memoria: es el momento de comprobar que la pantalla, el wifi, el sonido y sus programas funcionan, sin cambiar nada en su PC.

Para saber más: [Instalación en la wiki](wiki:Installation).
