#ifndef ROOTBEER_H
#define ROOTBEER_H

#include <stdio.h>
#include <assert.h>
#include <stdlib.h>
#include <dirent.h>
#include <string.h>
#include <libgen.h>
#include <sys/stat.h>
#include <unistd.h>
#include <stdarg.h>
#include <errno.h>
#include <limits.h>

// Utility functions (which are assert friendly)
int rb_create_dir(char *path);
int rb_copy_file(const char *src, const char *dst);
char **rb_recurse_files(const char *path, int *count);

void rb_fatal(const char *format, ...);

#endif // ROOTBEER_H

