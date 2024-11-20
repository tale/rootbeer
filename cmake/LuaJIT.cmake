# Called from our CMakeLists.txt
# This function handles finding or building LuaJIT

option(LUAJIT_SYSTEM "Force using system luajit" OFF)
if(LUAJIT_SYSTEM)
	message(STATUS "LUAJIT_SYSTEM is ON, will fail if LuaJIT is not found")
	find_package(LuaJIT REQUIRED)
else()
	find_package(LuaJIT QUIET)
endif()

if(LuaJIT_FOUND)
	message(STATUS "Found LuaJIT: ${LUAJIT_LIBRARIES}")
	message(STATUS "Found lua.h: ${LUAJIT_INCLUDE_DIR}/lua.h")
else()
	message(STATUS "LuaJIT not found, cloning and building from source")
	message(STATUS "LuaJIT will be statically linked to ${PROJECT}")
	include(BuildLuaJIT)
endif()

