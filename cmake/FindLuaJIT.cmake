# This is invoked when searching for LuaJIT via find_package
# It just does some custom work to allow a few variables
# such as LUAJIT_INCLUDE_DIR and LUAJIT_LIBRARIES to be specified

find_path(LUAJIT_INCLUDE_DIR
    NAMES luajit.h
    PATHS
        ENV LUAJIT_INCLUDE_DIR
        /usr/include
        /usr/local/include
        /opt/homebrew/include
    DOC "Path to LuaJIT include directory"
)

find_library(LUAJIT_LIBRARIES
    NAMES luajit luajit-5.1
    PATHS
        ENV LUAJIT_LIBRARY
        /usr/lib
        /usr/local/lib
        /opt/homebrew/lib
    DOC "Path to LuaJIT library"
)

include(FindPackageHandleStandardArgs)
find_package_handle_standard_args(LuaJIT
    REQUIRED_VARS LUAJIT_LIBRARIES LUAJIT_INCLUDE_DIR
)

mark_as_advanced(LUAJIT_LIBRARIES LUAJIT_INCLUDE_DIR)
