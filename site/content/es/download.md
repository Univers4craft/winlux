---
title: Descargar Winlux
description: Obtenga la imagen de instalación de la última versión y el creador de memoria USB, y compruebe que su PC es adecuado.
---

{{download}}

Los archivos vienen directamente de la página de versiones publicadas del proyecto en GitHub. La imagen se llama, por ejemplo, `winlux-live-v0.2.19.iso`; ocupa unos 1,3 GB.

## Lo que necesita

| | Mínimo recomendado |
|---|---|
| Procesador | PC de 64 bits (x86-64) |
| Memoria | 4 GB |
| Disco | 24 GB libres (20 GB como mínimo absoluto) |
| Arranque | UEFI o BIOS heredada, Secure Boot activado o no |
| Pantalla | de 1024 × 600 hasta 4K |
| Memoria USB | 4 GB o más, para instalar en un PC real |

## Tres formas de probarlo

::: cards
### En una máquina virtual
La forma más segura de descubrir Winlux: nada toca su PC. Vea la sección [máquina virtual](#probarlo-en-una-maquina-virtual) más abajo.

### Desde una memoria USB, sin instalar
La sesión «Live» arranca Winlux en memoria: prueba la pantalla, la red y el sonido, y no se escribe nada en el disco. [Crear la memoria](page:guide-usb)

### Instalado en el PC
Solo en el disco, o junto a un Windows existente con un menú para elegir al arrancar. [Instalar Winlux](page:guide-install)
:::

## Probarlo en una máquina virtual

| Programa | Ajustes recomendados |
|---|---|
| virt-manager, GNOME Cajas (QEMU/KVM) | 4 GB de memoria, de 2 a 4 procesadores, disco de 24 GB, pantalla «VGA estándar» o QXL |
| VirtualBox | Tipo «Linux / Ubuntu (64-bit)», 4 GB, 24 GB, controlador gráfico VMSVGA |
| VMware | «Ubuntu de 64 bits», 4 GB, 24 GB |

Arranque la máquina virtual desde el archivo ISO y siga [la instalación](page:guide-install) como en un PC real. En Configuración › Pantalla, la opción «Seguir el tamaño de la VM» ajusta el escritorio a la ventana.

## Comprobar el archivo descargado

El creador de memoria USB comprueba la imagen por sí solo. Si la usa de otro modo, compare su suma SHA-256 con el archivo `.sha256` publicado junto a ella. En Linux:

```sh
sha256sum -c winlux-live-v0.2.19.iso.sha256
```

Las dos sumas deben coincidir; si no, vuelva a descargar la imagen.

> [!note]
> ¿Ya tiene Winlux instalado? No tiene nada que descargar aquí: las nuevas versiones se instalan desde [Configuración › Actualizaciones](page:guide-updates).
