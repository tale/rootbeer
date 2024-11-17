
# build and manage luajit
set(LUAJIT_INSTALL_PATH "${CMAKE_BINARY_DIR}/luajit/src/luajit-extern/")

# right now we don't support system or dynamic linking luajit
set(LUAJIT_EMBEDDED 1)

if(DEFINED LUAJIT_SYSTEM)
    message(FATAL_ERROR "dynamic linking system luajit not supported yet")
    # message(STATUS "LUAJIT_SYSTEM:${LUAJIT_SYSTEM}")
    # find_library(LUAJIT_SYSTEM_FOUND luajit)
    # message(STATUS ${LUAJIT_SYSTEM_FOUND})
    # include(FindPkgConfig)
    # pkg_check_modules(LUAJIT luajit)
    # pkg_check_modules(LUA lua)
    # message(STATUS "LUAJIT_PKGCONFIG_FOUND:${LUAJIT_PKGCONFIG_FOUND}")
    # list(APPEND ${LUAJIT_LIBRARIES} ${LUA_LIBRARIES})
else()
    # once we have system dynamic it will set these so we should support them now
    set(LUAJIT_LIBRARIES 
        luajit
        m  # required for static linking 
    )
    set(LUAJIT_INCLUDE_DIRS "${LUAJIT_INSTALL_PATH}/src/")

    include(ExternalProject)
    ExternalProject_Add(
        luajit-extern
        PREFIX "luajit"
        GIT_REPOSITORY "https://github.com/LuaJIT/LuaJIT"
        GIT_TAG "04dca7911ea255f37be799c18d74c305b921c1a6"
        CONFIGURE_COMMAND ""
        BUILD_COMMAND make
        INSTALL_COMMAND make PREFIX=${CMAKE_BINARY_DIR} install
        BUILD_IN_SOURCE true
    )

    add_custom_command(
        TARGET luajit-extern
        POST_BUILD
        COMMAND ${CMAKE_COMMAND} -E rename ${LUAJIT_INSTALL_PATH}/src/libluajit.a ${CMAKE_BINARY_DIR}/libluajit.a
        COMMAND ${CMAKE_COMMAND} -E rename ${LUAJIT_INSTALL_PATH}/src/libluajit.so ${CMAKE_BINARY_DIR}/libluajit.so
    )

    if(DEFINED LUAJIT_EMBEDDED)
        message(STATUS "LUAJIT_EMBEDDED:${LUAJIT_EMBEDDED}")
        add_custom_command(
            TARGET luajit-extern
            POST_BUILD
            COMMAND ${CMAKE_COMMAND} -E remove ${CMAKE_BINARY_DIR}/lib/*luajit*.so* ${CMAKE_BINARY_DIR}/libluajit.so
        ) 
    endif()
endif()

# maybe needs to have another definition for finding system luajit
add_custom_target(luajit-external
    DEPENDS luajit-extern
)


