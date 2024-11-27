#ifndef CLI_MODULE_H
#define CLI_MODULE_H

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <getopt.h>

// Main CLI entrypoint for the application, essentially what main will
// always pass itself through in order to actually understand what it needs
// to do for the lifecycle of its execution.

int rb_cli_main(const int argc, const char *argv[]);
void rb_cli_print_help();

// Stores all commands for easy lookup of name -> function
// Also stores a description, usage is handled by the command itself
typedef struct {
	const char *name;
	const char *description;
	void (*print_usage)();
	int (*func)(const int argc, const char *argv[]);
} rb_cli_cmd;

// Individual commands are taped together using CMake's build system
// so that all a command needs to do is globally define the struct
// and it will be added to the rb_cli_commands array at build time
//
// IMPORTANT: The struct name MUST be the same as the file name
// for the build system to properly add it to the array.
extern rb_cli_cmd *rb_cli_cmds[];

#endif // CLI_MODULE_H

