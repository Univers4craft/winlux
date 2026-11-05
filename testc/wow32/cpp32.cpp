// C++ 32 bits (#232) : exceptions DWARF de MinGW, iostream, conteneurs.
#include <iostream>
#include <stdexcept>
#include <string>
#include <vector>
#include <map>
int main() {
    int fails = 0;
    try { throw std::runtime_error("boum"); }
    catch (const std::exception &e) { if (std::string(e.what()) != "boum") fails = 1; }
    std::vector<int> v{3, 1, 2};
    std::map<std::string, int> m{{"a", 1}, {"b", 2}};
    if (v.size() != 3 || m["b"] != 2) fails = 2;
    std::cout << "C++ 32 bits " << 42 << " " << 2.5 << std::endl;
    if (!fails) std::cout << "C++ 32 bits : tout est correct" << std::endl;
    return fails;
}
