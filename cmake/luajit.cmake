
# build and manage luajit
include(ExternalProject)
ExternalProject_Add(
    luajit
    PREFIX "luajit"
    GIT_REPOSITORY "https://github.com/LuaJIT/LuaJIT"
    GIT_TAG "04dca7911ea255f37be799c18d74c305b921c1a6"
    CONFIGURE_COMMAND ""
    BUILD_COMMAND make
    INSTALL_COMMAND make PREFIX=${CMAKE_BINARY_DIR}/luajit/src/luajit install
    BUILD_IN_SOURCE true
)

