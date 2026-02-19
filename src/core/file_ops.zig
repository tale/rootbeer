const std = @import("std");
const zlua = @import("zlua");
const Lua = zlua.Lua;
const Context = @import("context.zig").Context;

/// Registry key for the loaded modules cache table (matches root.zig).
const loaded_key = "_ROOTBEER_LOADED";

/// Resolve a file path with ~ expansion, absolute pass-through, or CWD prepend.
/// Returns an allocator-owned null-terminated string.
pub fn resolvePath(allocator: std.mem.Allocator, path: []const u8) ![:0]u8 {
    if (path.len == 0) return error.InvalidPath;

    // Handle ~ expansion
    if (path[0] == '~' and (path.len == 1 or path[1] == '/')) {
        const home = std.c.getenv("HOME") orelse return error.HomeNotSet;
        const home_slice = std.mem.span(home);
        const suffix = if (path.len > 1) path[1..] else "";
        return std.fmt.allocPrintZ(allocator, "{s}{s}", .{ home_slice, suffix });
    }

    // Absolute paths pass through
    if (path[0] == '/') {
        return allocator.dupeZ(u8, path);
    }

    // Relative path: prepend CWD
    var cwd_buf: [std.fs.max_path_bytes]u8 = undefined;
    const cwd = std.c.getcwd(&cwd_buf, cwd_buf.len) orelse return error.CwdFailed;
    const cwd_slice = std.mem.span(cwd);
    return std.fmt.allocPrintZ(allocator, "{s}/{s}", .{ cwd_slice, path });
}

/// Recursively create parent directories for the given path.
fn createParentDirs(filepath: [:0]const u8) !void {
    const dir = std.fs.path.dirname(filepath) orelse return;
    var buf: [std.fs.max_path_bytes]u8 = undefined;

    // Walk the path creating each component
    var end: usize = 1; // skip leading /
    while (end <= dir.len) : (end += 1) {
        if (end < dir.len and dir[end] != '/') continue;

        const component = dir[0..end];
        if (component.len >= buf.len) return error.PathTooLong;
        @memcpy(buf[0..component.len], component);
        buf[component.len] = 0;
        const path_z: [*:0]const u8 = buf[0..component.len :0];

        const ret = std.c.mkdir(path_z, 0o775);
        if (ret != 0 and std.c.errno(ret) != .EXIST) return error.MkdirFailed;
    }
}

/// rb.link_file(from, to) — create a symlink from source to target.
/// `from` is relative to the script directory, `to` supports ~ expansion.
pub fn link_file(lua: *Lua) i32 {
    const ctx = Context.fromLua(lua);
    const from = lua.checkString(1);
    const to = lua.checkString(2);

    // Resolve 'from' relative to script_dir
    const from_path = std.fmt.allocPrintZ(ctx.allocator, "{s}/{s}", .{ ctx.script_dir, from }) catch {
        lua.raiseErrorStr("link_file: allocation failed", .{});
    };
    defer ctx.allocator.free(from_path);

    // Verify source is accessible
    if (std.c.access(from_path.ptr, std.c.F_OK | std.c.R_OK) != 0) {
        lua.raiseErrorStr("Cannot access source '%s'", .{from.ptr});
    }

    // Resolve 'to' with ~ expansion
    const resolved_to = resolvePath(ctx.allocator, to) catch {
        lua.raiseErrorStr("Failed to resolve target path '%s'", .{to.ptr});
    };
    defer ctx.allocator.free(resolved_to);

    if (ctx.dry_run) {
        std.debug.print("  link {s} -> {s}\n", .{ from_path, resolved_to });
        return 0;
    }

    // Check if symlink already exists and points to the right place
    var existing_target_buf: [std.fs.max_path_bytes]u8 = undefined;
    const readlink_ret = std.c.readlink(resolved_to.ptr, &existing_target_buf, existing_target_buf.len);
    if (readlink_ret > 0) {
        const len: usize = @intCast(readlink_ret);
        const existing_target = existing_target_buf[0..len];
        if (std.mem.eql(u8, existing_target, from_path)) {
            std.debug.print("  link {s} (unchanged)\n", .{resolved_to});
            return 0;
        }
        // Different target — remove existing symlink
        _ = std.c.unlink(resolved_to.ptr);
    } else {
        // readlink failed — check if something else exists at the target
        if (std.c.access(resolved_to.ptr, std.c.F_OK) == 0) {
            lua.raiseErrorStr("Target '%s' exists and is not a symlink", .{to.ptr});
        }
    }

    // Create parent directories
    createParentDirs(resolved_to) catch {
        lua.raiseErrorStr("Failed to create directories for '%s'", .{to.ptr});
    };

    // Create the symlink
    if (std.c.symlink(from_path.ptr, resolved_to.ptr) != 0) {
        lua.raiseErrorStr("Failed to symlink '%s' -> '%s'", .{ from.ptr, to.ptr });
    }

    std.debug.print("  link {s} -> {s}\n", .{ resolved_to, from_path });

    // Track source in static_inputs and target in generated
    const from_dup = ctx.allocator.dupe(u8, from) catch {
        lua.raiseErrorStr("link_file: allocation failed", .{});
    };
    ctx.static_inputs.append(ctx.allocator, from_dup) catch {
        ctx.allocator.free(from_dup);
        lua.raiseErrorStr("link_file: tracking failed", .{});
    };

    const to_dup = ctx.allocator.dupe(u8, resolved_to) catch {
        lua.raiseErrorStr("link_file: allocation failed", .{});
    };
    ctx.generated.append(ctx.allocator, to_dup) catch {
        ctx.allocator.free(to_dup);
        lua.raiseErrorStr("link_file: tracking failed", .{});
    };

    return 0;
}

/// rb.write_file(path, content) — write content to a file.
/// Path supports ~ expansion. Returns the resolved path string.
pub fn write_file(lua: *Lua) i32 {
    const ctx = Context.fromLua(lua);
    const path = lua.checkString(1);
    const content = lua.checkString(2);

    const filepath = resolvePath(ctx.allocator, path) catch {
        lua.raiseErrorStr("Failed to resolve path '%s'", .{path.ptr});
    };
    defer ctx.allocator.free(filepath);

    // Track in generated
    const path_dup = ctx.allocator.dupe(u8, filepath) catch {
        lua.raiseErrorStr("write_file: allocation failed", .{});
    };
    ctx.generated.append(ctx.allocator, path_dup) catch {
        ctx.allocator.free(path_dup);
        lua.raiseErrorStr("write_file: tracking failed", .{});
    };

    // Write the file
    const f = std.c.fopen(filepath.ptr, "w") orelse {
        lua.raiseErrorStr("Failed to open file '%s'", .{path.ptr});
    };
    defer _ = std.c.fclose(f);

    const written = std.c.fwrite(content.ptr, 1, content.len, f);
    if (written != content.len) {
        lua.raiseErrorStr("Failed to write to file '%s'", .{path.ptr});
    }

    // Return the resolved path
    _ = lua.pushString(filepath);
    return 1;
}

/// rb.file(path, content) — write content to a file with dry_run support.
/// Creates parent directories as needed.
pub fn file(lua: *Lua) i32 {
    const ctx = Context.fromLua(lua);
    const raw_path = lua.checkString(1);
    const content = lua.checkString(2);

    const filepath = resolvePath(ctx.allocator, raw_path) catch {
        lua.raiseErrorStr("Failed to resolve path '%s'", .{raw_path.ptr});
    };
    defer ctx.allocator.free(filepath);

    if (ctx.dry_run) {
        std.debug.print("  write {s} ({d} bytes)\n", .{ filepath, content.len });
        return 0;
    }

    // Track in generated
    const path_dup = ctx.allocator.dupe(u8, filepath) catch {
        lua.raiseErrorStr("file: allocation failed", .{});
    };
    ctx.generated.append(ctx.allocator, path_dup) catch {
        ctx.allocator.free(path_dup);
        lua.raiseErrorStr("file: tracking failed", .{});
    };

    // Create parent directories
    createParentDirs(filepath) catch {
        lua.raiseErrorStr("Failed to create directories for '%s'", .{raw_path.ptr});
    };

    // Write the file
    const f = std.c.fopen(filepath.ptr, "w") orelse {
        lua.raiseErrorStr("Failed to open '%s'", .{raw_path.ptr});
    };
    defer _ = std.c.fclose(f);

    const written = std.c.fwrite(content.ptr, 1, content.len, f);
    if (written != content.len) {
        lua.raiseErrorStr("Failed to write '%s'", .{raw_path.ptr});
    }

    std.debug.print("  write {s}\n", .{filepath});
    return 0;
}

/// rb.to_json(table) — serialize a Lua table to a JSON string.
pub fn to_json(lua: *Lua) i32 {
    lua.checkType(1, .table);

    const ctx = Context.fromLua(lua);
    const json_value = luaTableToJson(lua, ctx.allocator, 1) catch {
        lua.raiseErrorStr("to_json: failed to build JSON value", .{});
    };
    defer freeJsonValue(ctx.allocator, json_value);

    const json_string = std.json.Stringify.valueAlloc(ctx.allocator, json_value, .{}) catch {
        lua.raiseErrorStr("to_json: failed to serialize JSON", .{});
    };
    defer ctx.allocator.free(json_string);

    _ = lua.pushString(json_string);
    return 1;
}

/// Convert a Lua table at the given stack index to a std.json.Value.
fn luaTableToJson(lua: *Lua, allocator: std.mem.Allocator, index: i32) !std.json.Value {
    const abs_index = if (index < 0) lua.getTop() + index + 1 else index;
    var obj = std.json.ObjectMap.init(allocator);

    lua.pushNil();
    while (lua.next(abs_index)) {
        // Key is at -2, value at -1
        const key: []const u8 = switch (lua.typeOf(-2)) {
            .string => lua.toString(-2) catch {
                lua.pop(2);
                return error.InvalidKey;
            },
            .number => blk: {
                // Copy the key to convert to string without confusing next()
                lua.pushValue(-2);
                const s = lua.toString(-1) catch {
                    lua.pop(3);
                    return error.InvalidKey;
                };
                lua.pop(1);
                break :blk s;
            },
            else => {
                lua.pop(1);
                continue;
            },
        };

        const key_dup = try allocator.dupe(u8, key);
        errdefer allocator.free(key_dup);

        const value: std.json.Value = switch (lua.typeOf(-1)) {
            .string => blk: {
                const s = lua.toString(-1) catch {
                    allocator.free(key_dup);
                    lua.pop(1);
                    continue;
                };
                break :blk .{ .string = try allocator.dupe(u8, s) };
            },
            .number => blk: {
                const n = lua.toNumber(-1) catch {
                    allocator.free(key_dup);
                    lua.pop(1);
                    continue;
                };
                break :blk .{ .float = n };
            },
            .boolean => .{ .bool = lua.toBoolean(-1) },
            .table => try luaTableToJson(lua, allocator, -1),
            else => {
                allocator.free(key_dup);
                lua.pop(1);
                continue;
            },
        };

        try obj.put(key_dup, value);
        lua.pop(1); // pop value, keep key for next iteration
    }

    return .{ .object = obj };
}

/// Free a std.json.Value tree including all owned strings and nested objects.
fn freeJsonValue(allocator: std.mem.Allocator, value: std.json.Value) void {
    switch (value) {
        .string => |s| allocator.free(s),
        .object => |obj| {
            var o = obj;
            var it = o.iterator();
            while (it.next()) |entry| {
                allocator.free(entry.key_ptr.*);
                freeJsonValue(allocator, entry.value_ptr.*);
            }
            o.deinit();
        },
        .array => |arr| {
            var a = arr;
            for (a.items) |item| {
                freeJsonValue(allocator, item);
            }
            a.deinit();
        },
        else => {},
    }
}

/// rb.interpolate_table(table, func) — call func with table, return string result.
pub fn interpolate_table(lua: *Lua) i32 {
    lua.checkType(1, .table);
    lua.checkType(2, .function);

    lua.pushValue(2); // push function
    lua.pushValue(1); // push table as argument

    lua.protectedCall(.{ .results = 1 }) catch {
        const msg = lua.toString(-1) catch "(unknown error)";
        lua.pop(1);
        lua.raiseErrorStr("Error in interpolation function: %s", .{msg.ptr});
    };

    if (lua.typeOf(-1) != .string) {
        lua.raiseErrorStr("Interpolation function must return a string", .{});
    }

    return 1;
}

/// rb.register_module(name, table) — register a Lua table as "rootbeer.<name>"
/// in the custom require loaded cache.
pub fn register_module(lua: *Lua) i32 {
    const modname = lua.checkString(1);
    lua.checkType(2, .table);

    if (modname.len == 0) {
        lua.raiseErrorStr("Module name cannot be empty", .{});
    }

    // Build "rootbeer.<modname>"
    var fullname_buf: [256]u8 = undefined;
    const prefix = "rootbeer.";
    const total_len = prefix.len + modname.len;
    if (total_len >= fullname_buf.len) {
        lua.raiseErrorStr("Module name too long", .{});
    }
    @memcpy(fullname_buf[0..prefix.len], prefix);
    @memcpy(fullname_buf[prefix.len..][0..modname.len], modname);
    fullname_buf[total_len] = 0;
    const fullname: [:0]const u8 = fullname_buf[0..total_len :0];

    // Get the loaded cache table from registry
    _ = lua.getField(zlua.registry_index, loaded_key);

    // Check if module already exists
    const existing = lua.getField(-1, fullname);
    if (existing != .nil) {
        lua.pop(2); // pop existing value and loaded table
        lua.raiseErrorStr("Module '%s' already exists", .{fullname.ptr});
    }
    lua.pop(1); // pop nil

    // Store: loaded[fullname] = table
    lua.pushValue(2); // push the module table
    lua.setField(-2, fullname); // loaded[fullname] = table

    lua.pop(1); // pop loaded table
    return 0;
}

test {
    std.testing.refAllDecls(@This());
}
