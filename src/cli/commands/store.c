#include "cli_module.h"
#include "store_module.h"

int rb_cli_store_init() {
	rb_store_init_or_die();
	return 0;
}

int rb_cli_store_destroy() {
	rb_store_destroy();
	return 0;
}

// The store command is used to manage the revision store for the system.
// This includes initializing the store, creating new revisions, switching
// to a specific revision, and listing all revisions.
int rb_cli_store(const int argc, const char *argv[]) {
	if (strcmp(argv[1], "init") == 0) {
		return rb_cli_store_init();
	}

	if (strcmp(argv[1], "destroy") == 0) {
		return rb_cli_store_destroy();
	}

	return 0;
}
