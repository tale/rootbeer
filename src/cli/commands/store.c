#include "cli_module.h"

int rb_cli_store(const int argc, const char *argv[]) {
	// We can call our own getopt here later
	for (int i = 0; i < argc; i++) {
		printf("argv[%d] = %s\n", i, argv[i]);
	}

	return 0;
}
