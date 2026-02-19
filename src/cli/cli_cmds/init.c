#include "cli_module.h"
#include "rb_ctx.h"
#include <limits.h>
#include <sys/stat.h>
#include <errno.h>
#include <spawn.h>
#include <sys/wait.h>

#define RB_SOURCE_DIR "source"
#define RB_DEFAULT_MANIFEST "rootbeer.lua"

static int rb_git_clone(const char *url, const char *dest) {
	pid_t pid;
	extern char **environ;

	char *const args[] = {
		"git", "clone", (char *)url, (char *)dest, NULL
	};

	int status = posix_spawnp(&pid, "git", NULL, NULL, args, environ);
	if (status != 0) {
		fprintf(stderr, "error: failed to spawn git: %s\n", strerror(status));
		return 1;
	}

	int wait_status;
	waitpid(pid, &wait_status, 0);
	if (!WIFEXITED(wait_status) || WEXITSTATUS(wait_status) != 0) {
		fprintf(stderr, "error: git clone failed\n");
		return 1;
	}

	return 0;
}

void rb_cli_init_print_usage() {
	printf("Usage: rootbeer init [<github-user/repo> | <git-url>]\n");
	printf("\n");
	printf("Initializes the rootbeer source directory.\n");
	printf("If a repository is given, it will be cloned.\n");
	printf("\n");
	printf("Examples:\n");
	printf("  rootbeer init                    # Create empty source dir\n");
	printf("  rootbeer init tale/dotfiles      # Clone from GitHub\n");
	printf("  rootbeer init https://...        # Clone from any git URL\n");
}

int rb_cli_init_func(const int argc, const char *argv[]) {
	const char *home = getenv("HOME");
	if (home == NULL) {
		fprintf(stderr, "error: $HOME is not set\n");
		return 1;
	}

	char data_dir[PATH_MAX];
	snprintf(data_dir, sizeof(data_dir), "%s%s", home, RB_DATA_DIR_SUFFIX);

	char source_dir[PATH_MAX];
	snprintf(source_dir, sizeof(source_dir), "%s/%s", data_dir, RB_SOURCE_DIR);

	// Check if already initialized
	if (access(source_dir, F_OK) == 0) {
		fprintf(stderr, "error: rootbeer is already initialized at %s\n", source_dir);
		fprintf(stderr, "To re-initialize, remove it first:\n");
		fprintf(stderr, "  rm -rf %s\n", source_dir);
		return 1;
	}

	// Create data directory if needed
	if (access(data_dir, F_OK) != 0) {
		// Simple two-level mkdir
		char local_share[PATH_MAX];
		snprintf(local_share, sizeof(local_share), "%s/.local/share", home);
		mkdir(local_share, 0755); // ignore EEXIST
		mkdir(data_dir, 0755);
	}

	if (argc < 3) {
		// No repo argument — just create the source dir and a skeleton manifest
		if (mkdir(source_dir, 0755) != 0) {
			fprintf(stderr, "error: could not create %s: %s\n", source_dir, strerror(errno));
			return 1;
		}

		char manifest[PATH_MAX];
		snprintf(manifest, sizeof(manifest), "%s/%s", source_dir, RB_DEFAULT_MANIFEST);
		FILE *f = fopen(manifest, "w");
		if (f) {
			fprintf(f, "-- Rootbeer configuration\n");
			fprintf(f, "-- See https://github.com/tale/rootbeer for documentation\n");
			fprintf(f, "local rb = require(\"rootbeer\")\n");
			fprintf(f, "local d = rb.data()\n\n");
			fprintf(f, "-- rb.file(\"~/.zshrc\", \"export EDITOR=nvim\\n\")\n");
			fclose(f);
		}

		printf("Initialized empty rootbeer config at %s\n", source_dir);
		printf("Edit %s to get started\n", manifest);
		return 0;
	}

	// Build the git URL from the argument
	const char *repo_arg = argv[2];
	char git_url[512];

	if (strstr(repo_arg, "://") != NULL || strstr(repo_arg, "git@") != NULL) {
		// Full URL provided
		snprintf(git_url, sizeof(git_url), "%s", repo_arg);
	} else {
		// Short form: user/repo → https://github.com/user/repo.git
		snprintf(git_url, sizeof(git_url), "https://github.com/%s.git", repo_arg);
	}

	printf("Cloning %s into %s...\n", git_url, source_dir);
	if (rb_git_clone(git_url, source_dir) != 0) {
		return 1;
	}

	// Check for manifest
	char manifest[PATH_MAX];
	snprintf(manifest, sizeof(manifest), "%s/%s", source_dir, RB_DEFAULT_MANIFEST);
	if (access(manifest, F_OK) == 0) {
		printf("Initialized rootbeer from %s\n", git_url);
		printf("Run 'rb apply' to apply your configuration\n");
	} else {
		printf("Initialized rootbeer from %s\n", git_url);
		printf("Warning: no %s found in repository\n", RB_DEFAULT_MANIFEST);
		printf("Create %s to get started\n", manifest);
	}

	return 0;
}

rb_cli_cmd init = {
	"init",
	"Initialize rootbeer with an optional dotfiles repository",
	rb_cli_init_print_usage,
	rb_cli_init_func
};
