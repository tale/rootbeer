# Scans through all the CLI commands and generates an array of them
# This array is defined in cli_module.h and used throughout for all operations

# This is the output file that will be generated in our build directory
set(FILE "${CMAKE_BINARY_DIR}/generated/cmd_array.c")
set_source_files_properties(${FILE} PROPERTIES GENERATED TRUE)
set_property(DIRECTORY APPEND PROPERTY ADDITIONAL_MAKE_CLEAN_FILES ${FILE})

# Create the header file and ensure some basic content is present
# including the directory and setting strictness like VERBATIM
add_custom_command(
	OUTPUT ${FILE}
	COMMAND ${CMAKE_COMMAND} -E make_directory "${CMAKE_BINARY_DIR}/generated"
	COMMAND ${CMAKE_COMMAND} -E echo "// Auto-generated by CMake" > ${FILE}
	COMMAND ${CMAKE_COMMAND} -E echo_append "\#include " >> ${FILE}
	COMMAND ${CMAKE_COMMAND} -E echo "\"cli_module.h\"" >> ${FILE}
	COMMAND ${CMAKE_COMMAND} -E echo "" >> ${FILE}
	DEPENDS ${CMD_SOURCES}
	COMMENT "Generating CLI-commands table"
	VERBATIM
)

# Get all the CLI command files and generate the extern declarations
# Note: We expect each file to have a global variable named after the file
# which stores the rb_cli_cmd structure.
file(GLOB CMD_SOURCES "${CMAKE_SOURCE_DIR}/src/cli/commands/*.c")

foreach(CMD_SOURCE ${CMD_SOURCES})
	# We add a very basic check to ensure that the file contains
	# "rb_cli_cmd <filename>" to ensure that the file is a CLI command
	# but this is naive and is not foolproof.
	file(READ ${CMD_SOURCE} CMD_CONTENTS)
	if(NOT CMD_CONTENTS MATCHES "rb_cli_cmd[ \t]*[a-zA-Z0-9_]*[ \t]*=")
		message(FATAL_ERROR "${CMD_SOURCE} does not define a command")
	endif()

	get_filename_component(CMD_NAME ${CMD_SOURCE} NAME_WE)
	add_custom_command(
		OUTPUT ${FILE}
		COMMAND ${CMAKE_COMMAND} -E echo_append "extern rb_cli_cmd " >> ${FILE}
		COMMAND ${CMAKE_COMMAND} -E echo_append "${CMD_NAME}" >> ${FILE}
		COMMAND ${CMAKE_COMMAND} -E echo "$<SEMICOLON>" >> ${FILE}
		DEPENDS ${CMD_SOURCE}
		COMMENT "Registering CLI command: ${CMD_NAME}"
		APPEND
	)
endforeach()

# Create the actual array of commands with pointers to their static
# global locations once the entire thing gets linked together.
add_custom_command(
	OUTPUT ${FILE}
	COMMAND ${CMAKE_COMMAND} -E echo "" >> ${FILE}
	COMMAND ${CMAKE_COMMAND} -E echo "rb_cli_cmd *rb_cli_cmds[] = {" >> ${FILE}
	DEPENDS ${CMD_SOURCES}
	APPEND
)

foreach(CMD_SOURCE ${CMD_SOURCES})
	get_filename_component(CMD_NAME ${CMD_SOURCE} NAME_WE)
	add_custom_command(
		OUTPUT ${FILE}
		COMMAND ${CMAKE_COMMAND} -E echo_append "    " >> ${FILE}
		COMMAND ${CMAKE_COMMAND} -E echo_append "\&${CMD_NAME}" >> ${FILE}
		COMMAND ${CMAKE_COMMAND} -E echo "$<COMMA>" >> ${FILE}
		DEPENDS ${CMD_SOURCE}
		APPEND
	)
endforeach()

# Close the array and add a NULL at the end to make iteration easier
add_custom_command(
	OUTPUT ${FILE}
	COMMAND ${CMAKE_COMMAND} -E echo "    NULL" >> ${FILE}
	COMMAND ${CMAKE_COMMAND} -E echo "}$<SEMICOLON>" >> ${FILE}
	DEPENDS ${CMD_SOURCES}
	APPEND
)

# Setup the custom target and the library that will be used to link
add_custom_target(generate_commands DEPENDS ${FILE})
add_library(${PROJECT}_cli ${FILE})
add_dependencies(${PROJECT}_cli generate_commands)
target_include_directories(${PROJECT}_cli PRIVATE
	${CMAKE_SOURCE_DIR}/include
)
