#include "rootbeer.h"

void rb_fatal(const char *format, ...) {
	fprintf(stderr, "Fatal Error: ");

	va_list args;
	va_start(args, format);
	vfprintf(stderr, format, args);
	va_end(args);

	fprintf(stderr, "\n");
	exit(EXIT_FAILURE);
}

