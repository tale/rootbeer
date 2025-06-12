#include "cli_module.h"

void rb_cli_print_help() {
	puts("rootbeer: Deterministically manage your system using Lua!");
	puts("Usage: rootbeer <command> [options]");
	puts("Commands:");

	for (int i = 0; rb_cli_cmds[i] != NULL; i++) {
		rb_cli_cmd *cmd = rb_cli_cmds[i];
		printf("  %s: %s\n", cmd->name, cmd->description);
	}
}
