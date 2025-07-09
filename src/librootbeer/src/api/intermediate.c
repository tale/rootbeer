#include "rb_rootbeer.h"
#include "rb_helpers.h"
#include "rb_idlist.h"
#include "rb_ctx.h"
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <limits.h>
#include <sys/stat.h>
#include <errno.h>
#include <unistd.h>

static int rb_valid_intermediate_id(const char *id) {
	if (id == NULL || *id == '\0') {
		return 0;
	}

	if (strlen(id) > RB_MAX_INTERMEDIATE_ID_LENGTH) {
		return 0;
	}

	for (const char *p = id; *p; p++) {
		if (!(
			(*p >= 'a' && *p <= 'z') ||
			(*p >= 'A' && *p <= 'Z') ||
			(*p >= '0' && *p <= '9') ||
			*p == '_' || *p == '-' || *p == '.'
		)) {
			return 0;
		}
	}

	return 1;
}

FILE *rb_open_intermediate(rb_ctx_t *ctx, const char *id) {
	if (!rb_valid_intermediate_id(id)) {
		fprintf(stderr, "Invalid intermediate ID: %s\n", id);
		return NULL;
	}

	// TODO: Move this somewhere where it isn't constantly created
	char tmp_dir[PATH_MAX];
	snprintf(tmp_dir, sizeof(tmp_dir), "%s/.rb-tmp", ctx->script_dir);
	if (mkdir(tmp_dir, 0755) == -1 && errno != EEXIST) {
		perror("Failed to create temporary directory");
		return NULL;
	}

	char filename[PATH_MAX];
	snprintf(filename, sizeof(filename), "%s/rb_transform_%s", tmp_dir, id);
	int fd = open(filename, O_RDWR | O_CREAT | O_TRUNC, 0644);
	if (fd < 0) {
		perror("Failed to open intermediate file");
		return NULL;
	}

	char *abs_path = realpath(filename, NULL);
	if (abs_path == NULL) {
		perror("Failed to resolve absolute path for intermediate file");
		close(fd);
		return NULL;
	}

	char *rel_path = rb_canon_relative(ctx, abs_path);
	free(abs_path);
	if (rel_path == NULL) {
		close(fd);
		return NULL;
	}

	if (rb_idlist_add(&ctx->intermediates, id, rel_path) < 0) {
		free(rel_path);
		close(fd);
		return NULL;
	}

	free(rel_path);
	FILE *fp = fdopen(fd, "w+");
	if (fp == NULL) {
		close(fd);
		return NULL;
	}

	return fp;
}

const char *rb_get_intermediate(rb_ctx_t *ctx, const char *id) {
	if (!rb_valid_intermediate_id(id)) {
		fprintf(stderr, "Invalid intermediate ID: %s\n", id);
		return NULL;
	}

	return rb_idlist_get(&ctx->intermediates, id);
}
