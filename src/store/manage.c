#include "store_module.h"

void rb_store_init_or_die() {
	if (access(STORE_PATH, F_OK | R_OK) == 0) {
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
	if (mkdir(dirname(STORE_PATH), 0755) != 0) {
		printf("error: could not create store directory\n");
		exit(1);
	}

	if (mkdir(STORE_PATH, 0755) != 0) {
		printf("error: could not create store directory\n");
		exit(1);
	}
}

void rb_store_destroy() {
	if (access(STORE_PATH, F_OK | R_OK) != 0) {
		printf("error: store does not exist\n");
		exit(1);
	}

	int uperm = getuid();
	if (uperm != 0) {
		printf("error: must run as root to destroy store\n");
		exit(1);
	}

	if (rmdir(STORE_PATH) != 0) {
		printf("error: could not destroy store directory\n");
		exit(1);
	}

	printf("store destroyed\n");
}
