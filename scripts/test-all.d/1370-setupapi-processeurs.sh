# #1370 : classe Processor de setupapi (diagnostic de Paint.NET), classe
# sans appareil = ensemble vide.
step "setupapi : classe Processor depuis /proc/cpuinfo (#1370)" cargo test -q -p winlux -- classe_processor_depuis_cpuinfo
