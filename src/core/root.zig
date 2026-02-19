const std = @import("std");
const zlua = @import("zlua");
const Lua = zlua.Lua;

pub const Context = @import("context.zig").Context;
pub const file_ops = @import("file_ops.zig");
pub const primitives = @import("primitives.zig");

pub const data_dir_suffix = "/.local/share/rootbeer";

/// Registry key for the loaded modules cache table.
const loaded_key = "_ROOTBEER_LOADED";

/// Registry key for the search paths string.
const search_paths_key = "_ROOTBEER_PATHS";

pub const LuaVm = struct {
    lua: *Lua,
    allocator: std.mem.Allocator,

    pub fn init(allocator: std.mem.Allocator, io: std.Io) !LuaVm {
        const lua = try Lua.init(allocator);
        errdefer lua.deinit();

        lua.openLibs();
        primitives.register(lua);
        try setupRequire(lua, allocator, io);

        return .{
            .lua = lua,
            .allocator = allocator,
        };
    }

    pub fn deinit(self: *LuaVm) void {
        self.lua.deinit();
    }

    pub fn execFile(self: *LuaVm, path: []const u8, io: std.Io) !void {
        const cwd = std.Io.Dir.cwd();
        const source: [:0]u8 = cwd.readFileAllocOptions(
            io,
            path,
            self.allocator,
            .unlimited,
            .of(u8),
            0,
        ) catch |err| {
            std.debug.print("error: could not read '{s}': {any}\n", .{ path, err });
            return error.FileNotFound;
        };
        defer self.allocator.free(source);

        self.lua.loadString(source) catch |err| {
            std.debug.print("error: syntax error in '{s}': {any}\n", .{ path, err });
            if (err == error.LuaError) {
                const msg = self.lua.toString(-1) catch "(unknown error)";
                std.debug.print("  {s}\n", .{msg});
                self.lua.pop(1);
            }
            return error.LuaSyntax;
        };

        self.lua.protectedCall(.{ .results = zlua.mult_return }) catch |err| {
            const msg = self.lua.toString(-1) catch "(unknown error)";
            std.debug.print("error: runtime error in '{s}': {s}\n", .{ path, msg });
            self.lua.pop(1);
            return err;
        };
    }
};

/// Build the semicolon-separated search paths string.
fn buildSearchPaths(allocator: std.mem.Allocator, io: std.Io) ![]const u8 {
    const home = std.c.getenv("HOME") orelse {
        std.process.fatal("$HOME is not set", .{});
    };
    const home_slice = std.mem.span(home);

    const data_dir = try std.fmt.allocPrint(allocator, "{s}{s}", .{ home_slice, data_dir_suffix });
    defer allocator.free(data_dir);

    const is_debug = @import("builtin").mode == .Debug;
    if (is_debug) {
        const cwd_path = std.process.currentPathAlloc(io, allocator) catch {
            std.process.fatal("could not get current working directory", .{});
        };
        defer allocator.free(cwd_path);

        return try std.fmt.allocPrint(
            allocator,
            "{s}/lua/?.lua;{s}/lua/?/init.lua;{s}/lua/?.lua;{s}/lua/?/init.lua",
            .{ data_dir, data_dir, cwd_path, cwd_path },
        );
    } else {
        return try std.fmt.allocPrint(
            allocator,
            "{s}/lua/?.lua;{s}/lua/?/init.lua",
            .{ data_dir, data_dir },
        );
    }
}

/// Set up a custom `require` global for Luau.
/// Luau is sandboxed and doesn't have require/package built-in, so we
/// implement our own that searches the configured paths.
fn setupRequire(lua: *Lua, allocator: std.mem.Allocator, io: std.Io) !void {
    const search_paths = try buildSearchPaths(allocator, io);
    defer allocator.free(search_paths);

    // Store search paths in the registry
    _ = lua.pushString(search_paths);
    lua.setField(zlua.registry_index, search_paths_key);

    // Create the loaded modules cache table in the registry
    lua.newTable();
    lua.setField(zlua.registry_index, loaded_key);

    // Register our custom require function
    lua.pushFunction(zlua.wrap(luaRequire));
    lua.setGlobal("require");
}

/// Custom require implementation for Luau.
/// Called from Lua as: require("module.name")
fn luaRequire(lua: *Lua) i32 {
    const modname = lua.toString(1) catch {
        lua.raiseErrorStr("require: expected a string argument", .{});
    };

    // Check if the module is already loaded
    _ = lua.getField(zlua.registry_index, loaded_key); // push loaded table
    const cached_type = lua.getField(-1, modname);
    if (cached_type != .nil) {
        // Module already loaded — return cached value
        // Stack: [loaded_table, cached_value]
        lua.remove(-2); // remove loaded table, leave cached value
        return 1;
    }
    lua.pop(1); // pop nil
    // Stack: [loaded_table]

    // Get search paths from registry
    _ = lua.getField(zlua.registry_index, search_paths_key);
    const paths = lua.toString(-1) catch {
        lua.pop(1);
        lua.pop(1); // pop loaded table
        lua.raiseErrorStr("require: search paths not configured", .{});
    };
    lua.pop(1); // pop paths string
    // Stack: [loaded_table]

    // Convert dots to directory separators
    var mod_path_buf: [512]u8 = undefined;
    const modname_len = modname.len;
    if (modname_len >= mod_path_buf.len) {
        lua.pop(1); // pop loaded table
        lua.raiseErrorStr("require: module name too long", .{});
    }
    @memcpy(mod_path_buf[0..modname_len], modname);
    for (mod_path_buf[0..modname_len]) |*ch| {
        if (ch.* == '.') ch.* = '/';
    }
    const mod_path = mod_path_buf[0..modname_len];

    // Try each search path pattern
    var path_iter = std.mem.splitScalar(u8, paths, ';');
    while (path_iter.next()) |pattern| {
        // Replace '?' with the module path
        if (std.mem.indexOf(u8, pattern, "?")) |qmark_pos| {
            var file_path_buf: [1024]u8 = undefined;
            const prefix = pattern[0..qmark_pos];
            const suffix = pattern[qmark_pos + 1 ..];
            const total_len = prefix.len + mod_path.len + suffix.len;
            if (total_len >= file_path_buf.len) continue;

            @memcpy(file_path_buf[0..prefix.len], prefix);
            @memcpy(file_path_buf[prefix.len..][0..mod_path.len], mod_path);
            @memcpy(file_path_buf[prefix.len + mod_path.len ..][0..suffix.len], suffix);
            const file_path = file_path_buf[0..total_len];

            if (tryLoadFile(lua, file_path, modname)) {
                // Stack: [loaded_table, module_result]
                // Cache the result
                lua.pushValue(-1); // dup result
                lua.setField(-3, modname); // loaded[modname] = result
                lua.remove(-2); // remove loaded table
                return 1;
            }
        }
    }

    lua.pop(1); // pop loaded table
    lua.raiseErrorStr("module '%s' not found", .{modname.ptr});
}

/// Try to load and execute a Lua source file. Returns true if successful,
/// with the module's return value on top of the stack.
fn tryLoadFile(lua: *Lua, file_path: []const u8, chunkname: [:0]const u8) bool {
    _ = chunkname;

    // We need a null-terminated path for fopen.
    var path_buf: [1024]u8 = undefined;
    if (file_path.len >= path_buf.len) return false;
    @memcpy(path_buf[0..file_path.len], file_path);
    path_buf[file_path.len] = 0;
    const path_z: [*:0]const u8 = path_buf[0..file_path.len :0];

    const file = std.c.fopen(path_z, "r") orelse return false;
    defer _ = std.c.fclose(file);

    var content_buf: [64 * 1024]u8 = undefined;
    var total: usize = 0;
    while (total < content_buf.len - 1) {
        const n = std.c.fread(@ptrCast(&content_buf[total]), 1, content_buf.len - 1 - total, file);
        if (n == 0) break;
        total += n;
    }

    if (total == 0) return false;

    content_buf[total] = 0;
    const source: [:0]const u8 = content_buf[0..total :0];

    lua.loadString(source) catch return false;

    lua.protectedCall(.{ .results = 1 }) catch {
        const msg = lua.toString(-1) catch "(unknown error)";
        std.debug.print("require: error loading module: {s}\n", .{msg});
        lua.pop(1);
        return false;
    };

    // If the module returned nil, push true as a sentinel
    if (lua.typeOf(-1) == .nil) {
        lua.pop(1);
        lua.pushBoolean(true);
    }

    return true;
}

test {
    std.testing.refAllDecls(@This());
}
