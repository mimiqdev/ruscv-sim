# CMake project for the ruscv-sim SystemC host integration (dev-plan D9).
#
# The development image builds the pinned Accellera SystemC 3.0.2 kernel with
# C++17 and installs it to /opt/systemc, so a plain CMake configure finds it
# through CMAKE_PREFIX_PATH or an exported SYSTEMC_HOME. The ruscv-sim side is
# the C ABI static library built with `--features capi`; see README.md.
cmake_minimum_required(VERSION 3.16)

project(ruscv_sim_systemc LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

if(NOT CMAKE_BUILD_TYPE AND NOT CMAKE_CONFIGURATION_TYPES)
  set(CMAKE_BUILD_TYPE Release CACHE STRING "Build type" FORCE)
endif()

# A developer who exports SYSTEMC_HOME gets the installed package without
# repeating -DCMAKE_PREFIX_PATH on every configure.
if(NOT DEFINED SystemCLanguage_DIR AND DEFINED ENV{SYSTEMC_HOME})
  list(APPEND CMAKE_PREFIX_PATH "$ENV{SYSTEMC_HOME}")
endif()

# SystemC 3.0.2 is the contract pin: the system kernel and its TLM-2.0 headers
# ship in that one package, so the version check covers both.
find_package(SystemCLanguage 3.0.2 CONFIG REQUIRED)
if(NOT SystemCLanguage_VERSION MATCHES "^3\\.0\\.2(\\.|$)")
  message(FATAL_ERROR
    "SystemC 3.0.2 is required (dev-plan D9); found ${SystemCLanguage_VERSION}")
endif()
find_package(Threads REQUIRED)

# The C ABI static library. Its default location is the debug cargo profile
# directory the smoke script and the plain-C smoke use.
set(RUSCV_SIM_LIBRARY "${CMAKE_CURRENT_SOURCE_DIR}/../../target/debug/libruscv_sim.a"
    CACHE FILEPATH "libruscv_sim.a built with `cargo rustc --lib --features capi --crate-type staticlib`")
if(NOT EXISTS "${RUSCV_SIM_LIBRARY}")
  message(FATAL_ERROR
    "ruscv-sim C ABI library not found at ${RUSCV_SIM_LIBRARY}; build it with "
    "`cargo rustc --lib --features capi --crate-type staticlib`")
endif()

add_executable(systemc_smoke
  main.cpp
  ram_target.cpp
  ruscv_initiator.cpp
  uart_target.cpp)

target_include_directories(systemc_smoke PRIVATE
  "${CMAKE_CURRENT_SOURCE_DIR}"
  "${CMAKE_CURRENT_SOURCE_DIR}/../../include")

# SystemC's imported target contributes its headers as system includes, so
# -Werror applies to this project's sources only.
target_compile_options(systemc_smoke PRIVATE -Wall -Wextra -Werror)

target_link_libraries(systemc_smoke PRIVATE
  "${RUSCV_SIM_LIBRARY}"
  SystemC::systemc
  Threads::Threads
  ${CMAKE_DL_LIBS}
  m)
