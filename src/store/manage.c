#include "store_module.h"

// Creates the directory structure for the store.
// This is a simple directory structure with two directories:
// - store: contains the revisions of the store
// - _gen: contains the generated files
//
// This function will fail if the store already exists.
void rb_store_init_or_die() {
	char *store_gen_path = malloc(strlen(STORE_ROOT) + strlen("_gen") + 2);
	sprintf(store_gen_path, "%s/%s", STORE_ROOT, "_gen");

	char *store_rev_path = malloc(strlen(STORE_ROOT) + strlen("store") + 2);
	sprintf(store_rev_path, "%s/%s", STORE_ROOT, "store");

	if (access(store_rev_path, F_OK | R_OK) == 0) {
		printf("error: store already exists and is probably initialized\n");
		exit(1);
	}

	int uperm = getuid();
	if (uperm != 0) {
		printf("error: must run as root to initialize store\n");
		exit(1);
	}

	// We don't have a "mkdir_p" so we need to do this manually
	// But we will sanely assume that /opt exists on the system
	if (mkdir(STORE_ROOT, 0755) != 0) {
		printf("error: could not create store directory\n");
		exit(1);
	}

	if (mkdir(store_rev_path, 0755) != 0) {
		printf("error: could not create store rev directory\n");
		exit(1);
	}

	if (mkdir(store_gen_path, 0755) != 0) {
		printf("error: could not create store gen directory\n");
		exit(1);
	}
}

// Currently broken teardown function.
// This function will fail if the store does not exist.
void rb_store_destroy() {
	if (access(STORE_ROOT, F_OK | R_OK) != 0) {
		printf("error: store does not exist\n");
		exit(1);
	}

	int uperm = getuid();
	if (uperm != 0) {
		printf("error: must run as root to destroy store\n");
		exit(1);
	}

	// Yeah this does NOT work because I haven't recursed yet
	// TODO: Recurse and delete all files and directories *sigh*
	if (rmdir(STORE_ROOT) != 0) {
		printf("error: could not destroy store directory\n");
		exit(1);
	}

	printf("store destroyed\n");
}
