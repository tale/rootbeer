#include "store_module.h"
#include "rootbeer.h"

// Generic function used to copy context files to the store path
int rb_store_copy_files(rb_revision_t *rev) {
	char store_path[PATH_MAX];
	sprintf(store_path, "%s/store/%d", STORE_ROOT, rev->id);

	// Assert because this is entirely a developer error
	assert(access(store_path, F_OK) == 0);

	char cfg_dir[PATH_MAX];
	char ref_dir[PATH_MAX];

	sprintf(cfg_dir, "%s/%s", store_path, "cfg");
	sprintf(ref_dir, "%s/%s", store_path, "ref");

	// Copy the config files
	for (int i = 0; i < rev->cfg_filesc; i++) {
		char src[PATH_MAX];
		char dst[PATH_MAX];

		sprintf(src, "%s/%s", rev->pwd, rev->cfg_filesv[i]);
		sprintf(dst, "%s/%s", cfg_dir, rev->cfg_filesv[i]);

		if (rb_copy_file(src, dst) != 0) {
			return 1;
		}
	}

	// Copy the reference files
	for (int i = 0; i < rev->ref_filesc; i++) {
		char src[PATH_MAX];
		char dst[PATH_MAX];

		sprintf(src, "%s/%s", rev->pwd, rev->ref_filesv[i]);
		sprintf(dst, "%s/%s", ref_dir, rev->ref_filesv[i]);

		if (rb_copy_file(src, dst) != 0) {
			return 1;
		}
	}

	return 0;
}

// Exports our rb_revision_t to the actual store on disk
int rb_store_revision_to_disk(rb_revision_t *rev) {
	char store_path[PATH_MAX];
	sprintf(store_path, "%s/store/%d", STORE_ROOT, rev->id);

	// Assert because this is entirely a developer error
	assert(access(store_path, F_OK) != 0);

	// Create the directory for the revision
	if (mkdir(store_path, 0755) != 0) {
		return 1;
	}

	// Write the meta file
	char meta_path[PATH_MAX];
	sprintf(meta_path, "%s/_meta", store_path);
	FILE *m_file = fopen(meta_path, "w");

	// TODO: Debug log all these returns
	if (m_file == NULL) {
		return 1;
	}

	fprintf(m_file, "name=%s\n", rev->name);
	fprintf(m_file, "timestamp=%ld\n", rev->timestamp);
	fprintf(m_file, "cfg_files=");

	for (int i = 0; i < rev->cfg_filesc; i++) {
		fprintf(m_file, "%s", rev->cfg_filesv[i]);
		if (i != rev->cfg_filesc - 1) {
			fprintf(m_file, ",");
		}
	}

	fprintf(m_file, "\nref_files=");
	for (int i = 0; i < rev->ref_filesc; i++) {
		fprintf(m_file, "%s", rev->ref_filesv[i]);
		if (i != rev->ref_filesc - 1) {
			fprintf(m_file, ",");
		}
	}

	fprintf(m_file, "\n");
	fclose(m_file);

	return rb_store_copy_files(rev);
}


// Given a context from our lua execution, convert it into an rb_revision_t
// and then dump it into the store with the next available ID.
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

	if (rb_store_revision_to_disk(rev) != 0) {
		fprintf(stderr, "error: failed to store revision\n");
		return 1;
	}

	rb_store_set_current_revision(rev->id);
	return 0;
}

