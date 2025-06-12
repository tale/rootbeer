#include "store_module.h"

// Helper function to parse a comma-separated list of strings
// The revision _meta stores all ref and cfg files in this format
void rb_read_list(char ***list, int *count, char *line) {
	int c = 0;
	for (int i = 0; i < strlen(line); i++) {
		if (line[i] == ',') {
			c++;
		}
	}

	// If there are no commas, then there is only one element
	c++;
	*count = c;

	// Allocate the array
	assert(*list == NULL);
	assert(*count >= 0);

	*list = calloc(c, sizeof(char *));
	char *token = strtok(line, ",");
	int idx = 0;

	while (token != NULL && idx < c) {
		(*list)[idx] = malloc(strlen(token) + 1);
		strncpy((*list)[idx], token, strlen(token));
		token = strtok(NULL, ",");
		idx++;
	}
}

// Reads a revision from the store by its ID.
rb_revision_t *rb_store_get_revision_by_id(const int id) {
	char *revision_path = malloc(strlen(STORE_ROOT) + strlen("/store/") + 10 + 1);
	sprintf(revision_path, "%s/store/%d", STORE_ROOT, id);

	if (access(revision_path, F_OK | R_OK) != 0) {
		printf("error: revision does not exist?\n");
		free(revision_path);
		return NULL;
	}
	
	FILE *file = fopen(revision_path, "r");
	if (file == NULL) {
		printf("error: could not open revision file\n");
		free(revision_path);
		return NULL;
	}

	// Read the _meta file in the revision directory
	char *meta_path = malloc(strlen(revision_path) + strlen("/_meta") + 1);
	sprintf(meta_path, "%s/_meta", revision_path);

	FILE *meta_file = fopen(meta_path, "r");
	if (meta_file == NULL) {
		printf("error: could not open meta file\n");
		free(revision_path);
		free(meta_path);
		fclose(file);
		return NULL;
	}
	
	rb_revision_t *rev = malloc(sizeof(rb_revision_t));
	rev->id = id;

	// Read the meta file in this format
	// name: <name>
	// timestamp: <timestamp>
	// cfg_files: <cfg_files> (comma separated)
	// ref_files: <ref_files> (comma separated)
	char buffer[1024];

	// Read the name
	while (fgets(buffer, 1024, meta_file) != NULL) {
		if (strncmp(buffer, "name: ", 6) == 0) {
			rev->name = malloc(strlen(buffer) - 6);
			sscanf(buffer, "name: %s", rev->name);
			continue;
		}

		if (strncmp(buffer, "timestamp: ", 11) == 0) {
			sscanf(buffer, "timestamp: %ld", &rev->timestamp);
			continue;
		}

		if (strncmp(buffer, "cfg_files: ", 11) == 0) {
			rb_read_list(&rev->cfg_filesv, &rev->cfg_filesc, buffer + 11);
			continue;
		}

		if (strncmp(buffer, "ref_files: ", 11) == 0) {
			rb_read_list(&rev->ref_filesv, &rev->ref_filesc, buffer + 11);
			continue;
		}
	}
	
	free(revision_path);
	free(meta_path);
	fclose(file);
	fclose(meta_file);
	return rev;
}
