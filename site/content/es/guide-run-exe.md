---
title: Abrir un programa de Windows
description: Haga doble clic en un .exe, instale programas con su instalador y ajuste sus permisos.
---

## Simplemente, doble clic

Haga doble clic en un archivo `.exe` en el Explorador o en el escritorio: se abre como en Windows. No hay nada que instalar ni configurar antes.

![El recorrido de Winlux: abrir un programa de Windows](img:lancer-un-exe.jpg "El recorrido del primer arranque muestra cómo abrir un programa de Windows.")

## Instalar un programa

1. Descargue el instalador desde el sitio del editor (`setup.exe`, `installer.exe` o `.msi`).
2. Haga doble clic en él y siga sus pasos, como en Windows.
3. El programa aparece en el menú Inicio. Para quitarlo, abra **Aplicaciones instaladas**.

Se admiten los instaladores más comunes (MSI, NSIS, Inno Setup). La **Tienda** de Winlux ofrece además programas de Windows y de Linux para instalar con un clic.

## Los permisos

La primera vez que un programa quiere usar la **cámara**, el **micrófono**, la **captura de pantalla** o sus **carpetas personales**, Winlux le pide permiso. Puede cambiar de opinión en cualquier momento en Configuración › Privacidad y seguridad, donde también se puede bloquear el acceso a la red programa por programa.

Cuando un instalador pide derechos de administrador, se abre una ventana de confirmación, como en Windows.

## Si un programa no se abre

| Mensaje | Qué significa |
|---|---|
| «Programa de Windows de 32 bits» | Hoy solo se admiten programas de 64 bits. Busque la versión de 64 bits del programa. |
| «Función que falta» | El programa usa una función de Windows que Winlux todavía no ofrece. Infórmenos: es lo que nos dice qué escribir a continuación. |
| El programa se cierra sin decir nada | Abra el Visor de eventos desde el menú Inicio: allí está anotada la causa. |

Tras un fallo, Winlux le propone **informar del problema**: vea [Obtener ayuda](page:guide-help). La lista de programas ya probados está en la página [Aplicaciones probadas](page:apps).

Para saber más: [Compatibilidad de las aplicaciones](wiki:Compatibilite-des-applications).
