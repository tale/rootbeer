/**
 * @file rb_idlist.h
 * @brief Contains the definition of the `rb_idlist_t` structure
 *
 * General purpose list that associates IDs with paths.
 * This header defines the structure along with helper methods for managing
 * the `rb_idlist_t` type.
 */
#ifndef RB_IDLIST_H
#define RB_IDLIST_H

#include <stddef.h>

/**
 * @brief A structure to hold a list of paths associated to IDs.
 * This structure is used to associate intermediate files to IDs, allowing
 * them to later be referenced and retrieved efficiently.
 * This list is also capable of self-expansion to accommodate more entries.
 */
typedef struct {
	char **ids; //!< Array of strings (IDs).
	char **paths; //!< Array of paths.
	size_t count; //!< Number of strings in the list.
	size_t capacity; //!< Capacity of the array, to avoid reallocating too much.
} rb_idlist_t;

/**
 * Initializes an ID list with a specified initial capacity.
 *
 * @param list Pointer to the rb_idlist_t structure to initialize.
 * @param initial_capacity The initial capacity of the ID list.
 * @return 0 on success, or a non-zero error code on failure.
 */
int rb_idlist_init(rb_idlist_t *list, size_t initial_capacity);

/**
 * Adds a new ID and path pair to the list.
 * If the list is full, it will automatically expand its capacity.
 * IMPORTANT: This uses linear O(n) scanning for deduplication of IDs.
 *
 * @param list Pointer to the rb_idlist_t structure.
 * @param id The ID to add to the list.
 * @param str The string to add to the list.
 * @return 0 on success, or a non-zero error code on failure.
 */
int rb_idlist_add(rb_idlist_t *list, const char *id, const char *path);

/**
 * Retrieves the path associated with a given ID.
 * If the ID is not found, it returns NULL.
 * IMPORTANT: This uses linear O(n) scanning for ID lookup.
 *
 * @param list Pointer to the rb_idlist_t structure.
 * @param id The ID to look up.
 * @return The associated path if found, or NULL if not found.
 */
const char *rb_idlist_get(const rb_idlist_t *list, const char *id);

/**
 * Frees the memory allocated for the ID list.
 * IMPORTANT: This will also free all strings and IDs stored in the list.
 *
 * @param list Pointer to the rb_idlist_t structure to free.
 */
void rb_idlist_free(rb_idlist_t *list);

#endif // RB_IDLIST_H
