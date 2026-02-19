const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    const lua_dep = b.dependency("zlua", .{
        .target = target,
        .optimize = optimize,
        .lang = .luau,
    });

    // --- librootbeer (static library) ---
    const core_lib = b.addLibrary(.{
        .name = "rootbeer_core",
        .linkage = .static,
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/core/root.zig"),
            .target = target,
            .optimize = optimize,
            .imports = &.{
                .{ .name = "zlua", .module = lua_dep.module("zlua") },
            },
        }),
    });

    b.installArtifact(core_lib);

    // --- rb (CLI executable) ---
    const cli_exe = b.addExecutable(.{
        .name = "rb",
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/cli/main.zig"),
            .target = target,
            .optimize = optimize,
            .imports = &.{
                .{ .name = "zlua", .module = lua_dep.module("zlua") },
                .{ .name = "rootbeer", .module = core_lib.root_module },
            },
        }),
    });

    b.installArtifact(cli_exe);

    // --- Run step ---
    const run_cmd = b.addRunArtifact(cli_exe);
    run_cmd.step.dependOn(b.getInstallStep());
    if (b.args) |args| {
        run_cmd.addArgs(args);
    }

    const run_step = b.step("run", "Run the rootbeer CLI");
    run_step.dependOn(&run_cmd.step);
}
