#include "cli_module.h"

#define EXTERN_CMD(name) extern rb_cli_cmd name;
#define CMD_ENTRY(name) &name,

RB_CLI_COMMANDS(EXTERN_CMD)

rb_cli_cmd *rb_cli_cmds[] = {
	RB_CLI_COMMANDS(CMD_ENTRY)
	NULL
};
