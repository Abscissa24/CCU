cmake_minimum_required(VERSION 3.16)
project(cgd_utils LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

if(CMAKE_CXX_COMPILER_ID STREQUAL "GNU" OR CMAKE_CXX_COMPILER_ID STREQUAL "Clang")
    add_compile_options(-Wall -Wextra -Wpedantic -O2)
endif()

add_library(cgd_utils STATIC
    src/pgn_parser.cpp
    src/binary_reader.cpp
    src/tournament_engine.cpp
)

target_include_directories(cgd_utils PUBLIC
    ${CMAKE_CURRENT_SOURCE_DIR}/include
)

add_executable(cgd_cli
    src/main.cpp
)

target_link_libraries(cgd_cli PRIVATE cgd_utils)

# Install targets
install(TARGETS cgd_utils cgd_cli
    ARCHIVE DESTINATION lib
    RUNTIME DESTINATION bin
)

install(DIRECTORY include/
    DESTINATION include/cgd_utils
    FILES_MATCHING PATTERN "*.hpp"
)
