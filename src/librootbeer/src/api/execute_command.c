#include "rb_rootbeer.h"
#include <spawn.h>
#include <errno.h>
#include <sys/wait.h>
#include <stdio.h>
#include <string.h>

int rb_execute_command(rb_lua_t *ctx, const char *command, const char *args) {
	// Execute with posix_spawn
	pid_t pid;
	int status;
	extern char **environ;

	int execstat = posix_spawn(
		&pid,
		command,
		NULL, // No file actions
		NULL, // No spawn attributes
		(char *const[]){(char *)command, (char *)args, NULL}, // Arguments
		environ // Environment variables
	);

	if (execstat != 0) {
		printf("Error executing command '%s': %s\n", command, strerror(execstat));
		return -1;
	}

	// Wait for the command to finish
	int wait_status;
	if (waitpid(pid, &wait_status, 0) == -1) {
		printf("Error waiting for command '%s': %s\n", command, strerror(errno));
		return -1;
	}

	if (WIFEXITED(wait_status)) {
		status = WEXITSTATUS(wait_status);
	} else if (WIFSIGNALED(wait_status)) {
		status = WTERMSIG(wait_status);
	} else {
		status = -1; // Unknown status
	}

	printf("Command '%s' executed with status: %d\n", command, status);
	return status;
}
