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

// X-macro list of all CLI commands.
// To add a new command, add a X(name) entry here where `name` matches
// the global rb_cli_cmd struct defined in the corresponding source file.
#define RB_CLI_COMMANDS(X) \
	X(apply) \
	X(init) \
	X(store)

extern rb_cli_cmd *rb_cli_cmds[];

#endif // CLI_MODULE_H

