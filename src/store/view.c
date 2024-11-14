#include "store_module.h"

// Counts how many revisions are stored in the system.
// It's a simple file count of STORE_ROOT/store
int rb_store_get_revision_count() {
	char *store_path = malloc(strlen(STORE_ROOT) + strlen("/store") + 1);
	sprintf(store_path, "%s/store", STORE_ROOT);

	DIR *dir = opendir(store_path);
	if (dir == NULL) {
		return 0;
	}

	int count = 0;
	struct dirent *entry;
	while ((entry = readdir(dir)) != NULL) {
		// Ignore . and ..
		if (strcmp(entry->d_name, ".") == 0
			|| strcmp(entry->d_name, "..") == 0) {
			continue;
		}

		if (entry->d_type == DT_DIR) {
			count++;
		}
	}

	closedir(dir);
	return count;
}

// Returns all the revisions stored in the system.
rb_revision_t **rb_store_get_all(int count) {
	if (count == 0) {
		return NULL;
	}

	printf("count: %d\n", count);

	rb_revision_t **revs = malloc(count * sizeof(rb_revision_t *));
	if (revs == NULL) {
		return NULL;
	}

	for (int i = 0; i < count; i++) {
		rb_revision_t *rev = rb_store_get_revision_by_id(i);
		if (rev == NULL) {
			continue;
		}

		revs[i] = rev;
	}

	return revs;
}
