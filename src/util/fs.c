#include "rootbeer.h"

// Creates a directory but recursively creates parent directories
// Starts from an empty path and uses slashes instead of dirname(3)
// Recursively done by checking all parents and working from EEXIST upwards
int rb_create_dir(char *path) {
	char tmp[PATH_MAX];
	snprintf(tmp, sizeof(tmp), "%s", path);
	size_t len = strlen(tmp);

	// Drop trailing slash
	if (tmp[len - 1] == '/') {
		tmp[len - 1] = '\0';
	}

	char *end = NULL;
	for (end = tmp + 1; *end; end++) {
		if (*end == '/') {
			*end = '\0'; // Terminate so we have a path component

			if (mkdir(tmp, 0775) != 0 && errno != EEXIST) {
				fprintf(stderr, "error: could not create directory\n");
				fprintf(stderr, "error: %s\n", strerror(errno));
				return 1;
			}

			*end = '/'; // Restore the slash
		}
	}

	if (mkdir(path, 0775) != 0 && errno != EEXIST) {
		fprintf(stderr, "error: could not create directory\n");
		fprintf(stderr, "error: %s\n", strerror(errno));
		return 1;
	}

	return 0;
}

int rb_copy_file(const char *src, const char *dst) {
	FILE *src_file = fopen(src, "r");
	if (src_file == NULL) {
		printf("error: could not open source file\n");
		return 1;
	}

	if (rb_create_dir(dirname((char *)dst)) != 0) {
		fclose(src_file);
		return 1;
	}

	FILE *dst_file = fopen(dst, "w");
	if (dst_file == NULL) {
		printf("error: could not open destination file\n");
		fclose(src_file);
		return 1;
	}

	char buffer[4096];
	size_t bytes;
	while ((bytes = fread(buffer, 1, sizeof(buffer), src_file)) > 0) {
		fwrite(buffer, 1, bytes, dst_file);
	}

	fclose(src_file);
	fclose(dst_file);
	return 0;
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
