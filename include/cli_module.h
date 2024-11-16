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

// Individual commands
int rb_cli_store(const int argc, const char *argv[]);
int rb_cli_apply(const int argc, const char *argv[]);

#endif // CLI_MODULE_H

