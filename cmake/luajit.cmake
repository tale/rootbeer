
# build and manage luajit
set(LUAJIT_INSTALL_PATH "${CMAKE_BINARY_DIR}/luajit/src/luajit-lib/")

include(ExternalProject)
ExternalProject_Add(
    luajit-lib
    PREFIX "luajit"
    GIT_REPOSITORY "https://github.com/LuaJIT/LuaJIT"
    GIT_TAG "04dca7911ea255f37be799c18d74c305b921c1a6"
    CONFIGURE_COMMAND ""
    BUILD_COMMAND make
    INSTALL_COMMAND make PREFIX=${CMAKE_BINARY_DIR} install
    BUILD_IN_SOURCE true
)

add_custom_command(
    TARGET luajit-lib
    POST_BUILD
    COMMAND ${CMAKE_COMMAND} -E rename ${LUAJIT_INSTALL_PATH}/src/libluajit.a ${CMAKE_BINARY_DIR}/libluajit.a
    COMMAND ${CMAKE_COMMAND} -E rename ${LUAJIT_INSTALL_PATH}/src/libluajit.so ${CMAKE_BINARY_DIR}/libluajit.so
)

if(DEFINED LUAJIT_EMBEDDED)
    message(STATUS "LUAJIT_EMBEDDED:${LUAJIT_EMBEDDED}")
    add_custom_command(
        TARGET luajit-lib
        POST_BUILD
        COMMAND ${CMAKE_COMMAND} -E remove ${CMAKE_BINARY_DIR}/lib/*luajit*.so* ${CMAKE_BINARY_DIR}/libluajit.so
    )
endif()

