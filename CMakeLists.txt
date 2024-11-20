# Minimum version is 3.19 to support Apple Silicon
cmake_minimum_required(VERSION 3.19...3.31)
set(PROJECT rootbeer)

project(
	${PROJECT}
	VERSION 0.0.0
	DESCRIPTION "Deterministically manage your system using Lua!"
	HOMEPAGE_URL "https://github.com/tale/rootbeer"
	LANGUAGES C
)

# Support for clangd if you want autocomplete
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

include(cmake/luajit.cmake)

add_executable(${PROJECT}
    src/main.c
    src/config/lua_init.c
    src/config/lua_module.c
    src/config/functions/debug_test.c

    src/cli/parse.c
    src/cli/commands.c
    src/cli/commands/apply.c
    src/cli/commands/store.c

    src/store/create.c
    src/store/current.c
    src/store/manage.c
    src/store/read.c
    src/store/view.c

	src/util/fs.c
	src/util/log.c
)

# Enforce C99 without extensions and with the standard library
set_property(TARGET ${PROJECT} PROPERTY C_STANDARD 99)
set_property(TARGET ${PROJECT} PROPERTY C_STANDARD_REQUIRED ON)
set_property(TARGET ${PROJECT} PROPERTY C_EXTENSIONS OFF)

# Nothing should be linking to the rootbeer executable
target_include_directories(${PROJECT} PUBLIC
	"${CMAKE_CURRENT_SOURCE_DIR}/include"
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

