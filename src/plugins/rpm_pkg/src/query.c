#include <solv/queue.h>
#include <solv/pooltypes.h>
#include <stdio.h>
#include <solv/solver.h>
#include <solv/transaction.h>
#include "rpm_pkg.h"

void query_dnf_packages(char **packages, size_t count) {
	Pool *pool = load_all_solv_repos(DNF5_CACHE);
	Queue job;
	queue_init(&job);

	for (size_t i = 0; i < count; i++) {
		const char *pkg = packages[i];
		if (!pkg || pkg[0] == '\0') {
			continue;
		}

		Id name = pool_str2id(pool, pkg, 1);
		if (!name) {
			fprintf(stderr, "Package '%s' not found in the pool.\n", pkg);
			continue;
		}

		queue_push2(&job, SOLVER_INSTALL | SOLVER_SOLVABLE_NAME, name);
	}

	Solver *solver = solver_create(pool);
	solver_solve(solver, &job);
	Transaction *t = solver_create_transaction(solver);
	if (!t) {
		fprintf(stderr, "Failed to create transaction.\n");
		solver_free(solver);
		pool_free(pool);
		return;
	}

	Queue installs;
	queue_init(&installs);
	transaction_installedresult(t, &installs);

	for (int i = 0; i < installs.count; i++) {
		Id id = installs.elements[i];
		Solvable *s = pool_id2solvable(pool, id);

		if (s) {
			printf("Package: %s\n", pool_id2str(pool, s->name));
			printf("Version: %s\n", pool_id2str(pool, s->evr));
			printf("Arch: %s\n", pool_id2str(pool, s->arch));
		} else {
			fprintf(stderr, "Solvable for ID %d not found.\n", id);
		}
	}

	queue_free(&installs);
	solver_free(solver);
	pool_free(pool);
	queue_free(&job);
}
