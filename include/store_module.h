#ifndef STORE_MODULE_H
#define STORE_MODULE_H

#include <assert.h>
#include <time.h>
#include <unistd.h>
#include <dirent.h>
#include <stdlib.h>
#include <string.h>
#include <stdio.h>
#include <libgen.h>
#include <sys/stat.h>

#define STORE_ROOT "/opt/rootbeer"

// This struct is basically what an entire revision looks like
// in our store. As a basic explanation, we say that a revision
// contains some basic metadata, config data, and all of the
// "side effects" that are generated by the revision, such as
// IO related things like external files.
typedef struct {
	int id; // Revisions count up from 0
	char *name; // Name of the revision (optional)
	time_t timestamp; // Unix timestamp
	
	char **cfg_filesv; // Array of config file paths
	int cfg_filesc; // Number of config files
	
	char **ref_filesv; // Array of reference file paths
	int ref_filesc; // Number of reference files
} rb_revision_t;

// The way we actually store revision data is like so:
// STORE_ROOT/store/<id>_<name_or_unnamed>_<unixtimestamp>
// - cfg: stores all the lua-config files used to generate the revision
// - ref: stores all the reference files used to generate the revision
// - _meta: contains all the metadata in a newline separated file
//
// Some side things:
// - STORE_ROOT/_current: contains the number of the current revision
// - STORE_ROOT/_gen: generated files from the current revision

int rb_store_get_revision_count();
rb_revision_t **rb_store_get_all(int count);

void rb_store_init_or_die();
void rb_store_destroy();

int rb_dump_revision(rb_revision_t *revision);

rb_revision_t *rb_store_get_current_revision();
rb_revision_t *rb_store_get_revision_by_id(const int id);
int rb_store_set_current_revision(const int id);

#endif // STORE_MODULE_H
