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

int rb_cli_store_read(int id) {
	rb_revision_t *rev;
	if (id < 0) {
		rev = rb_store_get_current_revision();
	} else {
		rev = rb_store_get_revision_by_id(id);
	}

	if (rev == NULL) {
		printf("Revision not found.\n");
		return 1;
	}

	printf("Revision %d\n", rev->id);
	printf("Name: %s\n", rev->name);
	printf("Timestamp: %s", asctime(localtime(&rev->timestamp)));
	printf("Config files count: %d\n", rev->cfg_filesc);
	printf("Reference files count: %d\n", rev->ref_filesc);

	for (int i = 0; i < rev->cfg_filesc; i++) {
		if (rev->cfg_filesv == NULL) {
			printf("Config file %d: NULL\n", i);
			continue;
		}
		printf("Config file %d: %s\n", i, rev->cfg_filesv[i]);
	}

	for (int i = 0; i < rev->ref_filesc; i++) {
		printf("Reference file %d: %s\n", i, rev->ref_filesv[i]);
	}
	return 0;
}

int rb_cli_store_list() {
	int count = rb_store_get_revision_count();
	if (count == 0) {
		printf("No revisions found.\n");
		return 0;
	}

	rb_revision_t **revs = rb_store_get_all(count);
	for (int i = 0; i < count; i++) {
		rb_revision_t *rev = revs[i];
		char time_buf[64];

		strftime(
			time_buf, sizeof(time_buf),
			"%Y-%m-%d %H:%M:%S", localtime(&rev->timestamp)
		);

		printf("[%d] %s (%s)\n", rev->id, rev->name, time_buf);
	}

	return 0;
}

// The store command is used to manage the revision store for the system.
// This includes initializing the store, creating new revisions, switching
// to a specific revision, and listing all revisions.
int rb_cli_store(const int argc, const char *argv[]) {
	if (strcmp(argv[2], "init") == 0) {
		return rb_cli_store_init();
	}

	if (strcmp(argv[2], "destroy") == 0) {
		return rb_cli_store_destroy();
	}

	if (strcmp(argv[2], "list") == 0) {
		return rb_cli_store_list();
	}

	if (strcmp(argv[2], "read") == 0) {
		if (argc < 4) {
			// Return the current revision if no id is provided.
			return rb_cli_store_read(-1);
		}

		int id;
		int res = sscanf(argv[3], "%d", &id);
		if (res != 1) {
			printf("Invalid revision id.\n");
			return 1;
		}

		return rb_cli_store_read(id);
	}

	return 0;
}
