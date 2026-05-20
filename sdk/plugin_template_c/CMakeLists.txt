cmake_minimum_required(VERSION 3.16)
project(my_plugin C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

add_executable(my_plugin plugin.c)
target_include_directories(my_plugin PRIVATE ${CMAKE_SOURCE_DIR}/../)

# Expect a WASI-compatible clang toolchain:
#   cmake -B build -DCMAKE_TOOLCHAIN_FILE=path/to/wasi-sdk.cmake -G Ninja
#   cmake --build build
target_link_options(my_plugin PRIVATE
    -Wl,--no-entry
    -Wl,--export=plugin_required_api_major
    -Wl,--export=plugin_required_api_minor
    -Wl,--export=plugin_init
    -Wl,--export=plugin_deinit
    -Wl,--export=plugin_on_enter
    -Wl,--export=plugin_on_exit
    -nostdlib
)

set_target_properties(my_plugin PROPERTIES SUFFIX ".wasm")
