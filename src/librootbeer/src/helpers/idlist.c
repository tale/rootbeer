#include "rb_idlist.h"
#include <stdlib.h>
#include <string.h>

int rb_idlist_init(rb_idlist_t *list, size_t initial_capacity) {
	list->count = 0;
	list->capacity = initial_capacity;
	list->ids = malloc(initial_capacity * sizeof(char *));
	list->paths = malloc(initial_capacity * sizeof(char *));
	if (!list->ids || !list->paths) {
		return -1;
	}

	return 0;
}

/**
 * Resize the ID list to a new capacity.
 *
 * @param list Pointer to the ID list.
 * @return 0 on success, -1 on failure.
 */
static int rb_idlist_resize(rb_idlist_t *list) {
	// 4 is a safe default capacity for small lists.
	size_t new_capacity = list->capacity == 0 ? 4 : list->capacity * 2;
	char **new_ids = realloc(list->ids, new_capacity * sizeof(char *));
	char **new_paths = realloc(list->paths, new_capacity * sizeof(char *));
	if (!new_ids || !new_paths) {
		free(new_ids);
		free(new_paths);
		return -1;
	}

	list->ids = new_ids;
	list->paths = new_paths;
	list->capacity = new_capacity;
	return 0;
}

int rb_idlist_add(rb_idlist_t *list, const char *id, const char *path) {
	for (size_t i = 0; i < list->count; i++) {
		// Update the path if the ID already exists.
		if (strcmp(list->ids[i], id) == 0) {
			free(list->paths[i]);
			list->paths[i] = strdup(path);
			if (!list->paths[i]) {
				return -1;
			}

			return 0;
		}
	}

	if (list->count >= list->capacity) {
		if (rb_idlist_resize(list) < 0) {
			return -1;
		}
	}

	list->ids[list->count] = strdup(id);
	if (!list->ids[list->count]) {
		return -1;
	}

	list->paths[list->count] = strdup(path);
	if (!list->paths[list->count]) {
		free(list->ids[list->count]);
		return -1;
	}

	list->count++;
	return 0;
}

const char *rb_idlist_get(const rb_idlist_t *list, const char *id) {
	if (list == NULL || list->ids == NULL) {
		return NULL;
	}

	for (size_t i = 0; i < list->count; i++) {
		if (strcmp(list->ids[i], id) == 0) {
			return list->paths[i];
		}
	}

	return NULL;
}

void rb_idlist_free(rb_idlist_t *list) {
	if (list == NULL) {
		return;
	}

	for (size_t i = 0; i < list->count; i++) {
		free(list->ids[i]);
		free(list->paths[i]);
	}

	free(list->ids);
	free(list->paths);

	list->ids = NULL;
	list->paths = NULL;
	list->count = 0;
	list->capacity = 0;
}
