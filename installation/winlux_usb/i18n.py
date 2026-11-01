"""Textes de l'outil en six langues (français, anglais, allemand, espagnol, italien, portugais)."""

import locale
import os

LANGS = ("fr", "en", "de", "es", "it", "pt")

# Chaque entrée : (fr, en, de, es, it, pt)
T = {
    "title": ("Créateur de clé USB Winlux", "Winlux USB Creator", "Winlux USB-Stick-Ersteller",
              "Creador de USB de Winlux", "Creatore di chiavette USB Winlux", "Criador de pen USB Winlux"),
    "subtitle": (
        "Prépare une clé USB qui démarre un PC et y installe Winlux.",
        "Prepares a USB stick that boots a PC and installs Winlux on it.",
        "Erstellt einen USB-Stick, der einen PC startet und Winlux darauf installiert.",
        "Prepara una memoria USB que arranca un PC e instala Winlux en él.",
        "Prepara una chiavetta USB che avvia un PC e vi installa Winlux.",
        "Prepara uma pen USB que arranca um PC e instala o Winlux nele."),
    "step_image": ("1. Image de Winlux", "1. Winlux image", "1. Winlux-Abbild", "1. Imagen de Winlux",
                   "1. Immagine di Winlux", "1. Imagem do Winlux"),
    "img_latest": ("Télécharger la dernière version", "Download the latest version",
                   "Neueste Version herunterladen", "Descargar la última versión",
                   "Scarica l'ultima versione", "Transferir a versão mais recente"),
    "img_checking": ("recherche de la dernière version…", "looking for the latest version…",
                     "neueste Version wird gesucht…", "buscando la última versión…",
                     "ricerca dell'ultima versione…", "a procurar a versão mais recente…"),
    "img_offline": ("Internet indisponible : choisissez un fichier ISO.",
                    "No Internet connection: choose an ISO file.",
                    "Keine Internetverbindung: Wählen Sie eine ISO-Datei.",
                    "Sin conexión a Internet: elija un archivo ISO.",
                    "Connessione Internet assente: scegli un file ISO.",
                    "Sem ligação à Internet: escolha um ficheiro ISO."),
    "img_file": ("Utiliser un fichier ISO déjà téléchargé", "Use an ISO file I already have",
                 "Bereits heruntergeladene ISO-Datei verwenden", "Usar un archivo ISO ya descargado",
                 "Usa un file ISO già scaricato", "Usar um ficheiro ISO já transferido"),
    "browse": ("Parcourir…", "Browse…", "Durchsuchen…", "Examinar…", "Sfoglia…", "Procurar…"),
    "no_file": ("Aucun fichier choisi", "No file selected", "Keine Datei ausgewählt",
                "Ningún archivo seleccionado", "Nessun file selezionato", "Nenhum ficheiro selecionado"),
    "iso_files": ("Images ISO", "ISO images", "ISO-Abbilder", "Imágenes ISO", "Immagini ISO", "Imagens ISO"),
    "step_usb": ("2. Clé USB", "2. USB stick", "2. USB-Stick", "2. Memoria USB", "2. Chiavetta USB", "2. Pen USB"),
    "no_usb": ("Aucune clé USB détectée. Branchez une clé de {min} ou plus.",
               "No USB stick found. Plug in a stick of {min} or more.",
               "Kein USB-Stick gefunden. Schließen Sie einen Stick mit mindestens {min} an.",
               "No se detecta ninguna memoria USB. Conecte una de {min} o más.",
               "Nessuna chiavetta USB rilevata. Inserisci una chiavetta da {min} o più.",
               "Nenhuma pen USB detetada. Ligue uma pen de {min} ou mais."),
    "refresh": ("Actualiser", "Refresh", "Aktualisieren", "Actualizar", "Aggiorna", "Atualizar"),
    "verify_option": ("Vérifier la clé après l'écriture (recommandé)", "Check the stick after writing (recommended)",
                      "Stick nach dem Schreiben prüfen (empfohlen)", "Comprobar la memoria tras escribir (recomendado)",
                      "Verifica la chiavetta dopo la scrittura (consigliato)", "Verificar a pen após a escrita (recomendado)"),
    "start": ("Créer la clé USB", "Create the USB stick", "USB-Stick erstellen", "Crear la memoria USB",
              "Crea la chiavetta USB", "Criar a pen USB"),
    "cancel": ("Annuler", "Cancel", "Abbrechen", "Cancelar", "Annulla", "Cancelar"),
    "close": ("Fermer", "Close", "Schließen", "Cerrar", "Chiudi", "Fechar"),
    "ready": ("Prêt.", "Ready.", "Bereit.", "Listo.", "Pronto.", "Pronto."),
    "confirm_title": ("Effacer la clé USB ?", "Erase the USB stick?", "USB-Stick löschen?",
                      "¿Borrar la memoria USB?", "Cancellare la chiavetta USB?", "Apagar a pen USB?"),
    "confirm_body": (
        "Tout le contenu de « {disk} » ({size}) va être effacé définitivement, puis Winlux sera écrit dessus.\n\nVérifiez qu'il s'agit bien de la bonne clé avant de continuer.",
        "Everything on \"{disk}\" ({size}) will be permanently erased, then Winlux will be written to it.\n\nMake sure this is the right stick before continuing.",
        "Der gesamte Inhalt von „{disk}“ ({size}) wird endgültig gelöscht und anschließend Winlux darauf geschrieben.\n\nPrüfen Sie, ob es der richtige Stick ist.",
        "Todo el contenido de «{disk}» ({size}) se borrará definitivamente y luego se escribirá Winlux.\n\nCompruebe que es la memoria correcta antes de continuar.",
        "Tutto il contenuto di «{disk}» ({size}) verrà cancellato definitivamente, poi vi sarà scritto Winlux.\n\nVerifica che sia la chiavetta giusta prima di continuare.",
        "Todo o conteúdo de «{disk}» ({size}) será apagado definitivamente e depois o Winlux será escrito.\n\nConfirme que é a pen certa antes de continuar."),
    "confirm_ok": ("Effacer et créer la clé", "Erase and create", "Löschen und erstellen", "Borrar y crear",
                   "Cancella e crea", "Apagar e criar"),
    "phase_release": ("Recherche de la dernière version…", "Looking for the latest version…",
                      "Neueste Version wird gesucht…", "Buscando la última versión…",
                      "Ricerca dell'ultima versione…", "A procurar a versão mais recente…"),
    "phase_download": ("Téléchargement de Winlux… {done} / {total}", "Downloading Winlux… {done} / {total}",
                       "Winlux wird heruntergeladen… {done} / {total}", "Descargando Winlux… {done} / {total}",
                       "Download di Winlux… {done} / {total}", "A transferir o Winlux… {done} / {total}"),
    "phase_checksum": ("Contrôle de l'image… {pct} %", "Checking the image… {pct} %", "Abbild wird geprüft… {pct} %",
                       "Comprobando la imagen… {pct} %", "Controllo dell'immagine… {pct} %",
                       "A verificar a imagem… {pct} %"),
    "phase_prepare": ("Préparation de la clé…", "Preparing the stick…", "Stick wird vorbereitet…",
                      "Preparando la memoria…", "Preparazione della chiavetta…", "A preparar a pen…"),
    "phase_auth": ("Autorisation administrateur…", "Administrator authorization…", "Administratorfreigabe…",
                   "Autorización de administrador…", "Autorizzazione amministratore…", "Autorização de administrador…"),
    "phase_write": ("Écriture sur la clé… {done} / {total} ({speed}/s)", "Writing to the stick… {done} / {total} ({speed}/s)",
                    "Schreiben auf den Stick… {done} / {total} ({speed}/s)", "Escribiendo en la memoria… {done} / {total} ({speed}/s)",
                    "Scrittura sulla chiavetta… {done} / {total} ({speed}/s)", "A escrever na pen… {done} / {total} ({speed}/s)"),
    "phase_verify": ("Vérification de la clé… {pct} %", "Checking the stick… {pct} %", "Stick wird geprüft… {pct} %",
                     "Comprobando la memoria… {pct} %", "Verifica della chiavetta… {pct} %", "A verificar a pen… {pct} %"),
    "done_title": ("Clé USB prête", "USB stick ready", "USB-Stick fertig", "Memoria USB lista",
                   "Chiavetta USB pronta", "Pen USB pronta"),
    "done_body": (
        "La clé Winlux est prête. Pour installer Winlux sur un PC :\n\n1. Branchez la clé et allumez le PC.\n2. Ouvrez le menu de démarrage (souvent F12, F11, F8 ou Échap) et choisissez la clé USB.\n3. Dans le menu Winlux, choisissez « Installer Winlux sur le disque ».",
        "The Winlux stick is ready. To install Winlux on a PC:\n\n1. Plug in the stick and turn on the PC.\n2. Open the boot menu (often F12, F11, F8 or Esc) and pick the USB stick.\n3. In the Winlux menu, choose \"Installer Winlux sur le disque\" (install to disk).",
        "Der Winlux-Stick ist fertig. So installieren Sie Winlux auf einem PC:\n\n1. Stick einstecken und PC einschalten.\n2. Startmenü öffnen (oft F12, F11, F8 oder Esc) und den USB-Stick wählen.\n3. Im Winlux-Menü „Installer Winlux sur le disque“ (auf Festplatte installieren) wählen.",
        "La memoria Winlux está lista. Para instalar Winlux en un PC:\n\n1. Conecte la memoria y encienda el PC.\n2. Abra el menú de arranque (a menudo F12, F11, F8 o Esc) y elija la memoria USB.\n3. En el menú de Winlux, elija «Installer Winlux sur le disque» (instalar en el disco).",
        "La chiavetta Winlux è pronta. Per installare Winlux su un PC:\n\n1. Inserisci la chiavetta e accendi il PC.\n2. Apri il menu di avvio (spesso F12, F11, F8 o Esc) e scegli la chiavetta USB.\n3. Nel menu Winlux, scegli «Installer Winlux sur le disque» (installa sul disco).",
        "A pen Winlux está pronta. Para instalar o Winlux num PC:\n\n1. Ligue a pen e ligue o PC.\n2. Abra o menu de arranque (muitas vezes F12, F11, F8 ou Esc) e escolha a pen USB.\n3. No menu Winlux, escolha «Installer Winlux sur le disque» (instalar no disco)."),
    "err_title": ("Erreur", "Error", "Fehler", "Error", "Errore", "Erro"),
    "err_read_image": ("Impossible de lire l'image : {error}", "Cannot read the image: {error}",
                       "Abbild kann nicht gelesen werden: {error}", "No se puede leer la imagen: {error}",
                       "Impossibile leggere l'immagine: {error}", "Não é possível ler a imagem: {error}"),
    "err_not_iso": ("Ce fichier n'est pas une image ISO.", "This file is not an ISO image.",
                    "Diese Datei ist kein ISO-Abbild.", "Este archivo no es una imagen ISO.",
                    "Questo file non è un'immagine ISO.", "Este ficheiro não é uma imagem ISO."),
    "err_not_hybrid": ("Cette image ISO ne peut pas démarrer depuis une clé USB. Utilisez l'ISO officielle de Winlux.",
                       "This ISO image cannot boot from a USB stick. Use the official Winlux ISO.",
                       "Dieses ISO-Abbild kann nicht von einem USB-Stick starten. Verwenden Sie die offizielle Winlux-ISO.",
                       "Esta imagen ISO no puede arrancar desde una memoria USB. Use la ISO oficial de Winlux.",
                       "Questa immagine ISO non può avviarsi da una chiavetta USB. Usa la ISO ufficiale di Winlux.",
                       "Esta imagem ISO não arranca a partir de uma pen USB. Use a ISO oficial do Winlux."),
    "err_checksum": ("L'image est abîmée (somme SHA-256 différente). Téléchargez-la à nouveau.",
                     "The image is damaged (SHA-256 mismatch). Download it again.",
                     "Das Abbild ist beschädigt (SHA-256 stimmt nicht). Laden Sie es erneut herunter.",
                     "La imagen está dañada (SHA-256 distinto). Descárguela de nuevo.",
                     "L'immagine è danneggiata (SHA-256 diverso). Scaricala di nuovo.",
                     "A imagem está danificada (SHA-256 diferente). Transfira-a novamente."),
    "err_release": ("Impossible de trouver la dernière version : {error}", "Cannot find the latest version: {error}",
                    "Neueste Version nicht gefunden: {error}", "No se encuentra la última versión: {error}",
                    "Impossibile trovare l'ultima versione: {error}", "Não foi possível encontrar a versão mais recente: {error}"),
    "err_download": ("Téléchargement impossible : {error}", "Download failed: {error}",
                     "Herunterladen fehlgeschlagen: {error}", "Error de descarga: {error}",
                     "Download non riuscito: {error}", "Falha na transferência: {error}"),
    "err_too_small": ("La clé est trop petite : il faut {need}, elle fait {have}.",
                      "The stick is too small: {need} needed, it has {have}.",
                      "Der Stick ist zu klein: {need} nötig, er hat {have}.",
                      "La memoria es demasiado pequeña: se necesitan {need}, tiene {have}.",
                      "La chiavetta è troppo piccola: servono {need}, ne ha {have}.",
                      "A pen é demasiado pequena: são necessários {need}, tem {have}."),
    "err_source_on_disk": ("L'image ISO est enregistrée sur cette clé. Copiez-la d'abord sur le PC.",
                           "The ISO image is stored on this stick. Copy it to the PC first.",
                           "Das ISO-Abbild liegt auf diesem Stick. Kopieren Sie es zuerst auf den PC.",
                           "La imagen ISO está en esta memoria. Cópiela primero al PC.",
                           "L'immagine ISO si trova su questa chiavetta. Copiala prima sul PC.",
                           "A imagem ISO está nesta pen. Copie-a primeiro para o PC."),
    "err_system_disk": ("Ce disque contient le système en cours : il ne peut pas être effacé.",
                        "This disk holds the running system: it cannot be erased.",
                        "Dieser Datenträger enthält das laufende System und kann nicht gelöscht werden.",
                        "Este disco contiene el sistema en uso: no se puede borrar.",
                        "Questo disco contiene il sistema in uso: non può essere cancellato.",
                        "Este disco contém o sistema em execução: não pode ser apagado."),
    "err_not_removable": ("Ce disque n'est pas une clé USB : il n'est pas proposé.",
                          "This disk is not a USB stick, so it is not offered.",
                          "Dieser Datenträger ist kein USB-Stick.", "Este disco no es una memoria USB.",
                          "Questo disco non è una chiavetta USB.", "Este disco não é uma pen USB."),
    "err_no_disk": ("La clé {path} a été débranchée.", "The stick {path} was unplugged.",
                    "Der Stick {path} wurde entfernt.", "La memoria {path} se ha desconectado.",
                    "La chiavetta {path} è stata scollegata.", "A pen {path} foi desligada."),
    "err_unmount": ("Impossible de libérer la clé ({mount}) : fermez les fenêtres qui l'utilisent puis réessayez.",
                    "Cannot release the stick ({mount}): close any window using it and try again.",
                    "Stick kann nicht freigegeben werden ({mount}): Schließen Sie alle Fenster, die ihn verwenden.",
                    "No se puede liberar la memoria ({mount}): cierre las ventanas que la usan e inténtelo de nuevo.",
                    "Impossibile liberare la chiavetta ({mount}): chiudi le finestre che la usano e riprova.",
                    "Não é possível libertar a pen ({mount}): feche as janelas que a usam e tente de novo."),
    "err_busy": ("La clé est encore utilisée par un programme. Fermez-le puis réessayez.",
                 "The stick is still in use by a program. Close it and try again.",
                 "Der Stick wird noch von einem Programm verwendet.", "Un programa todavía usa la memoria.",
                 "La chiavetta è ancora in uso da un programma.", "A pen ainda está a ser usada por um programa."),
    "err_admin": ("Les droits administrateur sont nécessaires pour écrire sur la clé.",
                  "Administrator rights are needed to write to the stick.",
                  "Zum Schreiben auf den Stick sind Administratorrechte nötig.",
                  "Se necesitan derechos de administrador para escribir en la memoria.",
                  "Servono i diritti di amministratore per scrivere sulla chiavetta.",
                  "São necessários direitos de administrador para escrever na pen."),
    "err_open": ("Impossible d'ouvrir la clé : {error}", "Cannot open the stick: {error}",
                 "Stick kann nicht geöffnet werden: {error}", "No se puede abrir la memoria: {error}",
                 "Impossibile aprire la chiavetta: {error}", "Não é possível abrir a pen: {error}"),
    "err_write": ("Erreur d'écriture : {error}. La clé est peut-être défectueuse ou a été débranchée.",
                  "Write error: {error}. The stick may be faulty or was unplugged.",
                  "Schreibfehler: {error}. Der Stick ist evtl. defekt oder wurde entfernt.",
                  "Error de escritura: {error}. La memoria puede estar dañada o se desconectó.",
                  "Errore di scrittura: {error}. La chiavetta potrebbe essere difettosa o scollegata.",
                  "Erro de escrita: {error}. A pen pode estar avariada ou foi desligada."),
    "err_read": ("Erreur de lecture : {error}", "Read error: {error}", "Lesefehler: {error}",
                 "Error de lectura: {error}", "Errore di lettura: {error}", "Erro de leitura: {error}"),
    "err_verify": ("La vérification a échoué : la clé est peut-être défectueuse. Essayez une autre clé.",
                   "Verification failed: the stick may be faulty. Try another stick.",
                   "Prüfung fehlgeschlagen: Der Stick ist evtl. defekt. Versuchen Sie einen anderen.",
                   "La comprobación falló: la memoria puede estar dañada. Pruebe otra.",
                   "Verifica non riuscita: la chiavetta potrebbe essere difettosa. Provane un'altra.",
                   "A verificação falhou: a pen pode estar avariada. Experimente outra."),
    "err_cancelled": ("Opération annulée. La clé n'est pas utilisable en l'état : recommencez pour la créer.",
                      "Cancelled. The stick is not usable as it is: start again to create it.",
                      "Abgebrochen. Der Stick ist so nicht nutzbar: Starten Sie erneut.",
                      "Cancelado. La memoria no se puede usar así: vuelva a empezar.",
                      "Annullato. La chiavetta non è utilizzabile così: ricomincia.",
                      "Cancelado. A pen não pode ser usada assim: recomece."),
    "err_auth": ("Autorisation refusée : la clé n'a pas été modifiée.", "Authorization refused: the stick was not changed.",
                 "Freigabe verweigert: Der Stick wurde nicht verändert.", "Autorización denegada: la memoria no se modificó.",
                 "Autorizzazione negata: la chiavetta non è stata modificata.", "Autorização recusada: a pen não foi alterada."),
    "err_internal": ("Erreur inattendue : {error}", "Unexpected error: {error}", "Unerwarteter Fehler: {error}",
                     "Error inesperado: {error}", "Errore imprevisto: {error}", "Erro inesperado: {error}"),
    "cancel_confirm": ("Arrêter maintenant ? La clé restera inutilisable tant qu'elle n'aura pas été recréée.",
                       "Stop now? The stick will stay unusable until it is created again.",
                       "Jetzt abbrechen? Der Stick bleibt unbrauchbar, bis er neu erstellt wird.",
                       "¿Detener ahora? La memoria no se podrá usar hasta volver a crearla.",
                       "Interrompere ora? La chiavetta resterà inutilizzabile finché non verrà ricreata.",
                       "Parar agora? A pen ficará inutilizável até ser criada novamente."),
    "password_prompt": ("Mot de passe administrateur pour écrire sur la clé :",
                        "Administrator password to write to the stick:",
                        "Administratorkennwort zum Schreiben auf den Stick:",
                        "Contraseña de administrador para escribir en la memoria:",
                        "Password di amministratore per scrivere sulla chiavetta:",
                        "Palavra-passe de administrador para escrever na pen:"),
    "units": ("o|Ko|Mo|Go|To", "B|KB|MB|GB|TB", "B|KB|MB|GB|TB", "B|KB|MB|GB|TB", "B|KB|MB|GB|TB", "B|KB|MB|GB|TB"),
}


def detect_lang():
    forced = os.environ.get("WINLUX_USB_LANG", "")
    candidates = [forced] if forced else []
    for var in ("LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"):
        if os.environ.get(var):
            candidates.append(os.environ[var])
    if os.name == "nt":
        try:
            import ctypes
            lcid = ctypes.windll.kernel32.GetUserDefaultUILanguage()
            candidates.append(locale.windows_locale.get(lcid, ""))
        except (AttributeError, OSError):
            pass
    try:
        candidates.append(locale.getlocale()[0] or "")
    except ValueError:
        pass
    for value in candidates:
        code = value.split(":")[0][:2].lower()
        if code in LANGS:
            return code
    return "fr"


LANG = detect_lang()


def set_lang(code):
    global LANG
    LANG = code if code in LANGS else "fr"


def tr(key, **fields):
    entry = T.get(key)
    if entry is None:
        return key
    text = entry[LANGS.index(LANG)]
    return text.format(**fields) if fields else text


def human_size(n):
    """Taille en unités décimales, comme sur l'emballage d'une clé (« 32,0 Go »)."""
    units = tr("units").split("|")
    value = float(n or 0)
    for unit in units:
        if value < 1000 or unit == units[-1]:
            text = f"{value:.0f} {unit}" if unit == units[0] else f"{value:.1f} {unit}"
            return text.replace(".", ",") if LANG in ("fr", "de", "es", "it", "pt") else text
        value /= 1000
    return str(n)


class _Blank(dict):
    def __missing__(self, key):
        return ""


def error_text(key, fields=None):
    """Message d'erreur traduit ; les tailles en octets sont rendues lisibles."""
    values = _Blank(fields or {})
    for name in ("need", "have"):
        if isinstance(values.get(name), int):
            values[name] = human_size(values[name])
    entry = T.get(key)
    if entry is None:
        return tr("err_internal", error=key)
    return entry[LANGS.index(LANG)].format_map(values)
