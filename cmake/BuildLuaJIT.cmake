# Builds LuaJIT as an ExternalProject and sets the LUAJIT_INCLUDE_DIR
# This means the final build will statically link against LuaJIT
include(ExternalProject)

set(BUILD_VARS "")
if(APPLE)
	# Minimum to macOS Catalina (honestly this is old enough)
	list(APPEND BUILD_VARS "MACOSX_DEPLOYMENT_TARGET=10.15")
endif()

string(JOIN " " BUILD_VARS ${BUILD_VARS})
ExternalProject_Add(luajit-extern
	PREFIX "luajit"
	GIT_REPOSITORY "https://github.com/LuaJIT/LuaJIT"
	GIT_TAG "04dca7911ea255f37be799c18d74c305b921c1a6"
	CONFIGURE_COMMAND ""
	BUILD_COMMAND ${BUILD_VARS} make -j${PROCESSOR_COUNT}
	INSTALL_COMMAND ""
	BUILD_IN_SOURCE true
)

ExternalProject_Get_Property(luajit-extern SOURCE_DIR)
set(LUAJIT_INCLUDE_DIR ${SOURCE_DIR}/src)
set(LUAJIT_LIBRARIES ${SOURCE_DIR}/src/libluajit.a)
mark_as_advanced(LUAJIT_INCLUDE_DIR LUAJIT_LIBRARIES)

add_library(LuaJIT STATIC IMPORTED)
set_target_properties(LuaJIT PROPERTIES
	IMPORTED_LOCATION ${LUAJIT_LIBRARIES}
	INTERFACE_INCLUDE_DIRECTORIES ${LUAJIT_INCLUDE_DIR}
)

add_dependencies(LuaJIT luajit-extern)
