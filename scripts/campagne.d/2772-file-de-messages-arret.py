# #2772 : DispatcherQueueController.ShutdownQueueAsync arrête la file, exécute
# les délégués en attente et lève ShutdownCompleted (Paint.NET à sa fermeture).
# Comportement gardé par le test unitaire
# dlls::coremessaging::tests::arret_de_la_file_par_shutdown_queue_async.
cm_2772 = (ROOT / "runtime" / "src" / "dlls" / "coremessaging.rs").read_text(encoding="utf-8")
ok("fn ctl_shutdown" in cm_2772 and "fn arret_de_la_file_par_shutdown_queue_async" in cm_2772,
   "DispatcherQueue : ShutdownQueueAsync arrête la file (#2772)")
