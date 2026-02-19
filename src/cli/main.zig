const std = @import("std");
const rootbeer = @import("rootbeer");

const Args = std.process.Args.Iterator;

const Command = struct {
    name: []const u8,
    desc: []const u8,
    run: *const fn (args: *Args, allocator: std.mem.Allocator, io: std.Io) anyerror!void,
};

const commands = [_]Command{
    .{ .name = "apply", .desc = "Apply a Lua configuration to your system", .run = cmdApply },
    .{ .name = "init", .desc = "Initialize rootbeer with an optional dotfiles repository", .run = cmdInit },
    .{ .name = "store", .desc = "Manage the revision store", .run = cmdStore },
    .{ .name = "version", .desc = "Print version information", .run = cmdVersion },
};

pub fn main(init: std.process.Init) !void {
    var args = Args.init(init.minimal.args);
    _ = args.next(); // skip program name

    const command_name = args.next() orelse {
        printUsage();
        return;
    };

    inline for (commands) |cmd| {
        if (std.mem.eql(u8, command_name, cmd.name)) {
            return cmd.run(&args, init.gpa, init.io);
        }
    }

    std.debug.print("error: unknown command '{s}'\n", .{command_name});
    printUsage();
}

fn printUsage() void {
    std.debug.print("rootbeer: Deterministically manage your system using Lua!\n", .{});
    std.debug.print("Usage: rb <command> [options]\n\n", .{});
    std.debug.print("Commands:\n", .{});
    inline for (commands) |cmd| {
        std.debug.print("  {s: <12} {s}\n", .{ cmd.name, cmd.desc });
    }
}

// --- apply ---

fn cmdApply(args: *Args, allocator: std.mem.Allocator, io: std.Io) !void {
    var dry_run = false;
    var script_path: ?[]const u8 = null;

    while (args.next()) |arg| {
        if (std.mem.eql(u8, arg, "--dry-run") or std.mem.eql(u8, arg, "-n")) {
            dry_run = true;
        } else if (arg.len > 0 and arg[0] == '-') {
            std.debug.print("error: unknown option '{s}'\n", .{arg});
            printApplyUsage();
            return;
        } else {
            script_path = arg;
        }
    }

    const path = script_path orelse {
        std.debug.print("error: 'apply' requires a file argument\n", .{});
        printApplyUsage();
        return;
    };

    var ctx = try rootbeer.Context.init(allocator, io, path, dry_run);
    defer ctx.deinit();

    var vm = try rootbeer.LuaVm.init(allocator, io);
    defer vm.deinit();

    ctx.registerInLua(vm.lua);
    try vm.execFile(path, io);
}

fn printApplyUsage() void {
    std.debug.print(
        \\Usage: rb apply [options] <file>
        \\
        \\Options:
        \\  -n, --dry-run  Show what would happen without making changes
        \\
    , .{});
}

// --- init ---

const data_dir_suffix = rootbeer.data_dir_suffix;
const source_dir_name = "source";
const default_manifest = "rootbeer.lua";

fn cmdInit(args: *Args, allocator: std.mem.Allocator, io: std.Io) !void {
    const repo_arg = args.next();

    const home = std.c.getenv("HOME") orelse {
        std.debug.print("error: $HOME is not set\n", .{});
        return error.HomeNotSet;
    };
    const home_slice = std.mem.span(home);

    const data_dir = try std.fmt.allocPrint(allocator, "{s}{s}", .{ home_slice, data_dir_suffix });
    defer allocator.free(data_dir);

    const source_dir = try std.fmt.allocPrint(allocator, "{s}/{s}", .{ data_dir, source_dir_name });
    defer allocator.free(source_dir);

    // Check if already initialized
    std.Io.Dir.accessAbsolute(io, source_dir, .{}) catch {
        // Does not exist — proceed
        return initCreate(allocator, io, data_dir, source_dir, repo_arg);
    };

    std.debug.print("error: rootbeer is already initialized at {s}\n", .{source_dir});
    std.debug.print("To re-initialize, remove it first:\n", .{});
    std.debug.print("  rm -rf {s}\n", .{source_dir});
}

fn initCreate(
    allocator: std.mem.Allocator,
    io: std.Io,
    data_dir: []const u8,
    source_dir: []const u8,
    repo_arg: ?[]const u8,
) !void {
    // Create data directory if needed
    std.Io.Dir.createDirAbsolute(io, data_dir, .default_dir) catch |err| switch (err) {
        error.PathAlreadyExists => {},
        else => {
            std.debug.print("error: could not create {s}: {any}\n", .{ data_dir, err });
            return err;
        },
    };

    if (repo_arg) |repo| {
        // Clone a repository
        const git_url = try buildGitUrl(allocator, repo);
        defer if (git_url.ptr != repo.ptr) allocator.free(git_url);

        std.debug.print("Cloning {s} into {s}...\n", .{ git_url, source_dir });
        try gitClone(allocator, io, git_url, source_dir);

        // Check for manifest
        const manifest = try std.fmt.allocPrint(allocator, "{s}/{s}", .{ source_dir, default_manifest });
        defer allocator.free(manifest);

        std.Io.Dir.accessAbsolute(io, manifest, .{}) catch {
            std.debug.print("Initialized rootbeer from {s}\n", .{git_url});
            std.debug.print("Warning: no {s} found in repository\n", .{default_manifest});
            std.debug.print("Create {s} to get started\n", .{manifest});
            return;
        };

        std.debug.print("Initialized rootbeer from {s}\n", .{git_url});
        std.debug.print("Run 'rb apply' to apply your configuration\n", .{});
    } else {
        // Create empty source dir with skeleton manifest
        std.Io.Dir.createDirAbsolute(io, source_dir, .default_dir) catch |err| {
            std.debug.print("error: could not create {s}: {any}\n", .{ source_dir, err });
            return err;
        };

        const manifest = try std.fmt.allocPrint(allocator, "{s}/{s}", .{ source_dir, default_manifest });
        defer allocator.free(manifest);

        const skeleton =
            "-- Rootbeer configuration\n" ++
            "-- See https://github.com/tale/rootbeer for documentation\n" ++
            "local rb = require(\"rootbeer\")\n" ++
            "local d = rb.data()\n\n" ++
            "-- rb.file(\"~/.zshrc\", \"export EDITOR=nvim\\n\")\n";

        const cwd = std.Io.Dir.cwd();
        cwd.writeFile(io, .{ .sub_path = manifest, .data = skeleton }) catch |err| {
            std.debug.print("error: could not create {s}: {any}\n", .{ manifest, err });
            return err;
        };

        std.debug.print("Initialized empty rootbeer config at {s}\n", .{source_dir});
        std.debug.print("Edit {s} to get started\n", .{manifest});
    }
}

fn buildGitUrl(allocator: std.mem.Allocator, repo: []const u8) ![]const u8 {
    // Full URL: contains "://" or starts with "git@"
    if (std.mem.indexOf(u8, repo, "://") != null or std.mem.startsWith(u8, repo, "git@")) {
        return repo;
    }
    // Short form: user/repo → https://github.com/user/repo.git
    return std.fmt.allocPrint(allocator, "https://github.com/{s}.git", .{repo});
}

fn gitClone(allocator: std.mem.Allocator, io: std.Io, url: []const u8, dest: []const u8) !void {
    var child = try std.process.spawn(io, .{
        .argv = &.{ "git", "clone", url, dest },
    });

    const term = try child.wait(io);

    switch (term) {
        .exited => |code| {
            if (code != 0) {
                std.debug.print("error: git clone failed (exit code {d})\n", .{code});
                return error.GitCloneFailed;
            }
        },
        else => {
            std.debug.print("error: git clone terminated abnormally\n", .{});
            return error.GitCloneFailed;
        },
    }

    _ = allocator;
}

// --- store ---

fn cmdStore(args: *Args, _: std.mem.Allocator, _: std.Io) !void {
    const subcmd = args.next() orelse {
        printStoreUsage();
        return;
    };

    if (std.mem.eql(u8, subcmd, "init")) {
        std.debug.print("store: init not yet implemented\n", .{});
    } else if (std.mem.eql(u8, subcmd, "destroy")) {
        std.debug.print("store: destroy not yet implemented\n", .{});
    } else if (std.mem.eql(u8, subcmd, "list")) {
        std.debug.print("store: list not yet implemented\n", .{});
    } else if (std.mem.eql(u8, subcmd, "read")) {
        const id_str = args.next();
        if (id_str) |id| {
            std.debug.print("store: read {s} not yet implemented\n", .{id});
        } else {
            std.debug.print("store: read (current) not yet implemented\n", .{});
        }
    } else {
        std.debug.print("error: unknown store command '{s}'\n", .{subcmd});
        printStoreUsage();
    }
}

fn printStoreUsage() void {
    std.debug.print(
        \\Usage: rb store <command>
        \\
        \\Commands:
        \\  init      Initialize the revision store
        \\  destroy   Destroy the revision store
        \\  list      List all revisions
        \\  read [id] Read a specific revision (or current)
        \\
    , .{});
}

// --- version ---

fn cmdVersion(_: *Args, _: std.mem.Allocator, _: std.Io) !void {
    std.debug.print("rootbeer v0.0.1\n", .{});
}
