#include <stddef.h>
#include <solv/pool.h>

#define MAX_REPOS 128
#define DNF5_CACHE "/var/cache/libdnf5"

typedef struct {
	char *name;
	char *evr;
	char *arch;
	char *repo;
	char *rpm_url;
} rpm_pkg_t;

void query_dnf_packages(char **packages, size_t count);
Pool *load_all_solv_repos(const char *solv_root);
