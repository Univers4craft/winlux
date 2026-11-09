"""Programme de test pygame win32 (#1346).

Ouvre une fenêtre 320×240, dessine un cercle et un texte, joue un son
synthétisé d'une demi-seconde, puis écrit « PYGAME32 OK » et rend 0.
"""
import array
import math
import sys

import pygame

print("PYGAME32 début", pygame.version.ver, flush=True)
pygame.mixer.pre_init(22050, -16, 1)
pygame.init()
ecran = pygame.display.set_mode((320, 240))
pygame.display.set_caption("pygame 32 bits")
ecran.fill((30, 30, 60))
pygame.draw.circle(ecran, (106, 61, 240), (160, 110), 60)
police = pygame.font.Font(None, 28)
ecran.blit(police.render("Winlux : pygame win32", True, (255, 255, 255)), (40, 190))
pygame.display.flip()

son_ok = False
if pygame.mixer.get_init():
    taux = pygame.mixer.get_init()[0]
    echantillons = array.array(
        "h",
        (int(8000 * math.sin(2 * math.pi * 440 * i / taux)) for i in range(taux // 2)),
    )
    pygame.mixer.Sound(buffer=echantillons.tobytes()).play()
    son_ok = True

fin = pygame.time.get_ticks() + 2000
while pygame.time.get_ticks() < fin:
    for evenement in pygame.event.get():
        if evenement.type == pygame.QUIT:
            fin = 0
    pygame.time.wait(20)

assert ecran.get_at((160, 110))[:3] == (106, 61, 240)
pygame.quit()
print("PYGAME32 OK", "son" if son_ok else "sans son", flush=True)
sys.exit(0)
