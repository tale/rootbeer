#include "rpm_pkg.h"
#include <dirent.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <solv/pool.h>
#include <solv/repo.h>
#include <solv/repo_solv.h>

static void populate_solv_files(const char *dir, char **found, size_t *count) {
	DIR *d = opendir(dir);
	if (!d) {
		return;
	}

	struct dirent *ent;
	while ((ent = readdir(d)) != NULL) {
		// Skip the usual troll stuff
		if (strcmp(ent->d_name, ".") == 0 || strcmp(ent->d_name, "..") == 0) {
			continue;
		}

		char path[PATH_MAX];
		snprintf(path, sizeof(path), "%s/%s", dir, ent->d_name);

		struct stat st;
		if (stat(path, &st) != 0) {
			continue;
		}

		// Recursing down the tree to a .solv file. I'm not sure how reliable
		// this is if there are duplicates BUTTTTTT lets pray for now.
		if (S_ISDIR(st.st_mode)) {
			populate_solv_files(path, found, count);
		} else if (S_ISREG(st.st_mode) && strstr(ent->d_name, ".solv") != NULL) {
			if (*count < MAX_REPOS) {
				found[*count] = strdup(path);
				(*count)++;
			}
		}
	}

	closedir(d);
}


Pool *load_all_solv_repos(const char *solv_root) {
	Pool *pool = pool_create();
	pool_setdisttype(pool, DISTTYPE_RPM);

	char *solv_paths[MAX_REPOS];
	size_t repos_count = 0;
	populate_solv_files(solv_root, solv_paths, &repos_count);
	if (repos_count == 0) {
		pool_free(pool);
		return NULL; // No solv files found
	}

	for (size_t i = 0; i < repos_count; i++) {
		if (solv_paths[i] == NULL) {
			continue;
		}

		FILE *fp = fopen(solv_paths[i], "r");
		if (fp == NULL) {
			free(solv_paths[i]);
			continue;
		}

		const char *basename = strrchr(solv_paths[i], '/');
		Repo *repo = repo_create(pool, basename ? basename + 1 : solv_paths[i]);
		if (repo_add_solv(repo, fp, 0) != 0) {
			fprintf(stderr, "Failed to add solv file: %s\n", solv_paths[i]);
		} else {
			repo_internalize(repo);
		}

		fclose(fp);
		free(solv_paths[i]);
	}

	pool_createwhatprovides(pool);
	return pool;
}
