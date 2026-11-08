/* Fibres des programmes 64 bits (#654) : le même test que les fibres x86
 * (testc/wow32/fiber32.c), compilé pour x64. GCC prend la lecture de
 * gs:[0x20] des en-têtes (GetCurrentFiber) pour un accès à l'adresse 0. */
#pragma GCC diagnostic ignored "-Warray-bounds"
#include "wow32/fiber32.c"
