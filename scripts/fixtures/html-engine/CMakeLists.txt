# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.16)
project(files_html_probe LANGUAGES C CXX)
if(NOT IS_DIRECTORY "${LITEHTML_SOURCE}")
  message(FATAL_ERROR "LITEHTML_SOURCE must name the verified pinned cache")
endif()
set(LITEHTML_BUILD_TESTING OFF CACHE BOOL "" FORCE)
set(LITEHTML_ENABLE_LINT OFF CACHE BOOL "" FORCE)
set(BUILD_SHARED_LIBS OFF CACHE BOOL "" FORCE)
add_subdirectory("${LITEHTML_SOURCE}" engine)

# Only accept the isolated patched copy, never an unmodified upstream source.
# The runner applies exclude-strtod.patch with --fuzz=0 before configuration.
get_target_property(engine_sources litehtml SOURCES)
list(FIND engine_sources "src/strtod.cpp" strtod_index)
list(FIND engine_sources "src/files_strtod_bridge.cpp" bridge_index)
if(NOT strtod_index EQUAL -1 OR bridge_index EQUAL -1)
  message(FATAL_ERROR "Expected explicit patched strtod source exclusion")
endif()

add_executable(html-engine-probe probe.cpp)
target_compile_features(html-engine-probe PRIVATE cxx_std_17)
target_compile_options(html-engine-probe PRIVATE -Wall -Wextra -Wpedantic)
target_link_libraries(html-engine-probe PRIVATE litehtml)
