# #1355 : DispatcherQueue.GetForCurrentThread passe par RoGetActivationFactory.
step "Fabrique WinRT de DispatcherQueue (#1355)" cargo test -q -p winlux -- fabrique_dispatcher_queue_get_for_current_thread
