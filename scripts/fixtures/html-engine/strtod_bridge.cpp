// SPDX-License-Identifier: MIT
// Only for this ephemeral, single-threaded executable: main fixes LC_NUMERIC=C.
// Replaces upstream's separately licensed UC/Sun strtod implementation.
#include <cstdlib>
namespace litehtml {
double t_strtod(const char* source, char** end) {
    return std::strtod(source, end);
}
} // namespace litehtml
