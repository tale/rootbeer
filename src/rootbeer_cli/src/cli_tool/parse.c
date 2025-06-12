#include "cli_module.h"

int rb_cli_main(const int argc, const char *argv[]) {
	if (argc < 2) {
		rb_cli_print_help();
		return 1;
	}

	for (int i = 0; rb_cli_cmds[i] != NULL; i++) {
		rb_cli_cmd *cmd = rb_cli_cmds[i];
		if (strcmp(argv[1], cmd->name) == 0) {
			// At this moment each of our commands have their own subcommands
			// so if they have 0 arguments we call cmd.print_usage() instead
			if (argc == 2) {
				cmd->print_usage();
				return 0;
			}

			return cmd->func(argc, argv);
		}
	}

	rb_cli_print_help();
	return 0;
}
