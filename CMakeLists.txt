
cmake_minimum_required(VERSION 3.20)
set(PROJECT rootbeer)
project(${PROJECT})

include(cmake/luajit.cmake)

set(SOURCES
    "src/main.c"
    "src/config/load.c"
    "src/config/module.c"
    "src/config/functions/debug_test.c"

    "src/cli/parse.c"
    "src/cli/commands.c"
    "src/cli/commands/compose.c"
    "src/cli/commands/store.c"

    "src/store/create.c"
    "src/store/current.c"
    "src/store/manage.c"
    "src/store/read.c"
    "src/store/view.c"
)

add_executable(${PROJECT}
    ${SOURCES}
)

target_include_directories(${PROJECT} PUBLIC
    "include/"
    "${LUAJIT_INCLUDE_DIRS}"
)

# required for linking with built externals
target_link_directories(${PROJECT} PUBLIC
     "${CMAKE_BINARY_DIR}"
     "${CMAKE_BINARY_DIR}/lib"
)

target_link_libraries(${PROJECT} PUBLIC
    "${LUAJIT_LIBRARIES}"
)

add_dependencies(${PROJECT} luajit-external)

