#include "rb_strlist.h"
#include <stdlib.h>
#include <string.h>

int rb_strlist_init(rb_strlist_t *list, size_t initial_capacity) {
	list->count = 0;
	list->capacity = initial_capacity;
	list->items = malloc(initial_capacity * sizeof(char *));
	if (!list->items) {
		return -1;
	}

	return 0;
}

/**
 * Resize the string list to a new capacity.
 *
 * @param list Pointer to the string list.
 * @return 0 on success, -1 on failure.
 */
static int rb_strlist_resize(rb_strlist_t *list) {
	// 4 is a safe default capacity for small lists.
	size_t new_capacity = list->capacity == 0 ? 4 : list->capacity * 2;
	char **new_list = realloc(list->items, new_capacity * sizeof(char *));
	if (!new_list) {
		return -1;
	}

	list->items = new_list;
	list->capacity = new_capacity;
	return 0;
}

int rb_strlist_add(rb_strlist_t *list, const char *str) {
	for (size_t i = 0; i < list->count; i++) {
		if (strcmp(list->items[i], str) == 0) {
			return 0;
		}
	}

	if (list->count >= list->capacity) {
		if (rb_strlist_resize(list) < 0) {
			return -1;
		}
	}

	char *copy = strdup(str);
	if (!copy) {
		return -1;
	}

	list->items[list->count++] = copy;
	return 0;
}

void rb_strlist_free(rb_strlist_t *list) {
	if (list == NULL) {
		return;
	}

	if (list->items) {
		for (size_t i = 0; i < list->count; i++) {
			free(list->items[i]);
		}

		free(list->items);
		list->items = NULL;
	}

	list->count = 0;
	list->capacity = 0;
}
