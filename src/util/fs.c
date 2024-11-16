#include "rootbeer.h"

// Creates a directory (or returns safely if it already exists)
// Return values are flipped so I can assert and be lazy
int rb_create_dir(const char *path) {
	if (access(path, F_OK | R_OK | W_OK) == 0) {
		return 1;
	}

	if (mkdir(path, 0755) != 0) {
		printf("error: could not create directory\n");
		printf("path: %s\n", path);
		return 0;
	}

	return 1;
}

char **rb_recurse_files(const char *path, int *count) {
	DIR *dir = opendir(path);
	if (dir == NULL) {
		printf("error: could not open directory\n");
		return NULL;
	}

	struct dirent *entry;
	int c = 0;
	while ((entry = readdir(dir)) != NULL) {
		if (entry->d_type == DT_REG) {
			c++;
		}
	}

	*count = c;
	if (c == 0) {
		return NULL;
	}

	char **files = calloc(c, sizeof(char *));
	if (files == NULL) {
		printf("error: could not allocate memory\n");
		return NULL;
	}

	rewinddir(dir);
	int idx = 0;
	while ((entry = readdir(dir)) != NULL) {
		if (entry->d_type == DT_REG) {
			files[idx] = malloc(strlen(entry->d_name) + 1);
			strcpy(files[idx], entry->d_name);
			idx++;
		}
	}

	closedir(dir);
	return files;
}
