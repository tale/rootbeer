#include "store_module.h"

// Returns the current revision.
rb_revision_t *rb_store_get_current_revision() {
	char *current_path = malloc(strlen(STORE_ROOT) + strlen("/_current") + 1);
	strcpy(current_path, STORE_ROOT);
	strcat(current_path, "/_current");

	char buffer[256];
	FILE *file = fopen(current_path, "r");
	if (file == NULL) {
		return NULL;
	}

	fgets(buffer, 256, file);
	fclose(file);

	int id;
	int res = sscanf(buffer, "%d", &id);
	if (res != 1) {
		return NULL;
	}

	return rb_store_get_revision_by_id(id);
}

// Sets the current revision.
int rb_store_set_current_revision(const int id) {
	FILE *file = fopen(STORE_ROOT "/_current", "w");
	if (file == NULL) {
		return 1;
	}

	fprintf(file, "%d", id);
	fclose(file);
	return 0;
}

// Returns the ID for the next revision.
int rb_store_next_id() {
	rb_revision_t *rev = rb_store_get_current_revision();
	if (rev == NULL) {
		return 0;
	}

	return rev->id + 1;
}
