
cmake_minimum_required(VERSION 3.20)
set(PROJECT rootbeer)
project(${PROJECT})

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
    "${CMAKE_BINARY_DIR}/luajit/src/luajit/src/"
)

target_link_directories(${PROJECT} PUBLIC
    "${CMAKE_BINARY_DIR}/luajit/src/luajit/lib"
    "${CMAKE_BINARY_DIR}/luajit/src/luajit/src"
)

target_link_libraries(${PROJECT} PUBLIC
    luajit
)

include(cmake/luajit.cmake)

