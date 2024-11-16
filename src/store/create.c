#include "store_module.h"

// General purpose function used to dump the cfg and ref files of a revision
int rb_dump_revision_files(rb_revision_t *revision, char *dir, char type) {
	// Make the directory
	char *files_path = malloc(
		strlen(STORE_ROOT) + strlen("/store") + 10 + strlen(dir) + 1
	);

	sprintf(files_path, "%s/store/%d/%s", STORE_ROOT, revision->id, dir);

	if (access(files_path, F_OK) == 0) {
		printf("error: directory already exists\n");
		free(files_path);
		return 1;
	}

	if (mkdir(files_path, 0755) != 0) {
		printf("error: could not create directory\n");
		free(files_path);
		return 1;
	}

	// Copy over the files
	char **files;
	int count;

	switch (type) {
		case 'c':
			files = revision->cfg_filesv;
			count = revision->cfg_filesc;
			break;
		case 'r':
			files = revision->ref_filesv;
			count = revision->ref_filesc;
			break;
		default:
			printf("error: invalid type\n");
			free(files_path);
			return 1;
	}

	for (int i = 0; i < count; i++) {
		char *file_path = malloc(strlen(files_path) + strlen(files[i]) + 1);
		sprintf(file_path, "%s/%s", files_path, files[i]);

		if (access(file_path, F_OK) == 0) {
			printf("error: file already exists\n");
			free(file_path);
			continue;
		}

		printf("copying %s to %s\n", files[i], file_path);
		FILE *file = fopen(file_path, "w");
		if (file == NULL) {
			printf("error: could not open file\n");
			free(file_path);
			continue;
		}

		fclose(file);
		free(file_path);
	}

	free(files_path);
	return 0;
}

// Dumps a revision to the store
int rb_dump_revision(rb_revision_t *revision) {
	assert(revision != NULL);
	assert(getuid() == 0);

	char *store_path = malloc(strlen(STORE_ROOT) + strlen("/store") + 10 + 1);
	sprintf(store_path, "%s/store/%d", STORE_ROOT, revision->id);
	printf("store path: %s\n", store_path);

	if (access(store_path, F_OK) == 0) {
		printf("error: revision already exists\n");
		free(store_path);
		return 1;
	}

	if (mkdir(store_path, 0755) != 0) {
		printf("error: could not create revision directory\n");
		free(store_path);
		return 1;
	}

	char *meta_path = malloc(strlen(store_path) + strlen("/_meta") + 1);
	sprintf(meta_path, "%s/_meta", store_path);

	FILE *meta_file = fopen(meta_path, "w");
	if (meta_file == NULL) {
		printf("error: could not open meta file\n");
		free(store_path);
		free(meta_path);
		return 1;
	}

	fprintf(meta_file, "name: %s\n", revision->name);
	fprintf(meta_file, "timestamp: %ld\n", revision->timestamp);

	fprintf(meta_file, "cfg_files: ");
	for (int i = 0; i < revision->cfg_filesc; i++) {
		fprintf(meta_file, "%s", basename(revision->cfg_filesv[i]));
		if (i != revision->cfg_filesc - 1) {
			fprintf(meta_file, ",");
		} else {
			fprintf(meta_file, "\n");
		}
	}

	fprintf(meta_file, "ref_files: ");
	for (int i = 0; i < revision->ref_filesc; i++) {
		fprintf(meta_file, "%s", basename(revision->ref_filesv[i]));
		if (i != revision->ref_filesc - 1) {
			fprintf(meta_file, ",");
		} else {
			fprintf(meta_file, "\n");
		}
	}

	fprintf(meta_file, "\n");
	fclose(meta_file);
	free(meta_path);

	// Copy over the files into the cfg and ref directories
	if (rb_dump_revision_files(revision, "cfg", 'c') != 0) {
		free(store_path);
		return 1;
	}

	if (rb_dump_revision_files(revision, "ref", 'r') != 0) {
		free(store_path);
		return 1;
	}

	return 0;
}

int rb_store_dump_revision(rb_lua_t *ctx) {
	assert(getuid() == 0); // We need to be root to dump a revision

	rb_revision_t *rev = malloc(sizeof(rb_revision_t));
	assert(rev != NULL);
	assert(ctx != NULL);

	rev->id = rb_store_next_id();
	rev->name = "test rev for now";
	rev->timestamp = time(NULL);

	// Revisions need full paths to the files, requiring we resolve the PWD
	rev->pwd = malloc(PATH_MAX);
	assert(rev->pwd != NULL);
	assert(realpath(ctx->config_root, rev->pwd) != NULL);

	int file_count = ctx->req_filesc + 1; // Include the entry point file
	rev->cfg_filesv = malloc(sizeof(char *) * file_count);
	rev->cfg_filesc = file_count;

	// Resolve the entry point file and add it to the cfg files
	char entry_point[PATH_MAX];
	realpath(ctx->config_file, entry_point);

	rev->cfg_filesv[0] = malloc(strlen(entry_point) - strlen(rev->pwd));
	sprintf(rev->cfg_filesv[0], "%s", entry_point + strlen(rev->pwd) + 1);

	// Resolve all the config file paths and trim off the PWD
	// since we are storing the hierarchy as-is on disk
	for (int i = 1; i < file_count; i++) {
		char cfg_file[PATH_MAX];
		realpath(ctx->req_filesv[i - 1], cfg_file);

		// Trim off the PWD (which is easy since we just skip strlen(rev->pwd))
		// The paths that are relative to the config path will always start
		// with the PWD anyways, so we just advance the array by the length
		rev->cfg_filesv[i] = malloc(strlen(cfg_file) - strlen(rev->pwd));
		sprintf(rev->cfg_filesv[i], "%s", cfg_file + strlen(rev->pwd) + 1);
	}

	// Print all revision data
	printf("revision id: %d\n", rev->id);
	printf("revision name: %s\n", rev->name);
	printf("revision timestamp: %s", ctime(&rev->timestamp));
	printf("revision pwd: %s\n", rev->pwd);
	printf("revision cfg files:\n");
	for (int i = 0; i < rev->cfg_filesc; i++) {
		printf("  %s\n", rev->cfg_filesv[i]);
	}

	/*rb_store_set_current_revision(rev->id);*/
	return 0;
	/*return rb_dump_revision(rev);*/
}

