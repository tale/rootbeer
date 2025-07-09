/**
 * @file rb_strlist.h
 * @brief Contains the definition of the rb_strlist_t structure.
 *
 * General purpose string list structure used in Rootbeer.
 * This header defines the structure along with helper methods for managing
 * the `rb_strlist_t` type.
 */
#ifndef RB_STRLIST_H
#define RB_STRLIST_H

#include <stddef.h>

/**
 * @brief A structure to hold a list of strings.
 * This structure is used to manage a list of strings that is reused for the
 * context storing any tracked files, such as Lua scripts, extra files, etc.
 * The list is capable of self-expanding to accommodate more strings as needed.
 */
typedef struct {
	char **items; //!< Array of strings.
	size_t count; //!< Number of strings in the list.
	size_t capacity; //!< Capacity of the array, to avoid reallocating too much.
} rb_strlist_t;

/**
 * Initializes a string list with a specified initial capacity.
 *
 * @param list Pointer to the rb_strlist_t structure to initialize.
 * @param initial_capacity The initial capacity of the string list.
 * @return 0 on success, or a non-zero error code on failure.
 */
int rb_strlist_init(rb_strlist_t *list, size_t initial_capacity);

/**
 * Adds a new string to the string list.
 * If the list is full, it will automatically expand its capacity.
 * IMPORTANT: This uses linear O(n) scanning for deduplication.
 *
 * @param list Pointer to the rb_strlist_t structure.
 * @param str The string to add to the list.
 * @return 0 on success, or a non-zero error code on failure.
 */
int rb_strlist_add(rb_strlist_t *list, const char *str);

/**
 * Frees the memory allocated for the string list.
 * IMPORTANT: This will also free all strings stored in the list.
 *
 * @param list Pointer to the rb_strlist_t structure to free.
 */
void rb_strlist_free(rb_strlist_t *list);

#endif // RB_STRLIST_H
