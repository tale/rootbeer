const std = @import("std");
const zlua = @import("zlua");
const Lua = zlua.Lua;
const Context = @import("context.zig").Context;

/// Lua-callable function entry for module registration.
pub const Entry = struct {
    name: [:0]const u8,
    func: zlua.CFn,
};

/// All rootbeer primitives. Add new entries here for task 13.
pub const entries = [_]Entry{
    .{ .name = "emit", .func = zlua.wrap(emit) },
    .{ .name = "line", .func = zlua.wrap(line) },
    .{ .name = "data", .func = zlua.wrap(data) },
    .{ .name = "ref_file", .func = zlua.wrap(refFile) },
};

/// Register the "rb" global table with all primitives.
pub fn register(lua: *Lua) void {
    lua.newTable();
    for (entries) |entry| {
        lua.pushFunction(entry.func);
        lua.setField(-2, entry.name);
    }
    lua.setGlobal("rb");
}

/// rb.emit(str) — append string to context output buffer.
fn emit(lua: *Lua) i32 {
    const ctx = Context.fromLua(lua);
    const str = lua.checkString(1);
    ctx.output.appendSlice(ctx.allocator, str) catch {
        lua.raiseErrorStr("emit: failed to append to output buffer", .{});
    };
    return 0;
}

/// rb.line(str) — append string + newline to context output buffer.
fn line(lua: *Lua) i32 {
    const ctx = Context.fromLua(lua);
    const str = lua.checkString(1);
    ctx.output.appendSlice(ctx.allocator, str) catch {
        lua.raiseErrorStr("line: failed to append to output buffer", .{});
    };
    ctx.output.append(ctx.allocator, '\n') catch {
        lua.raiseErrorStr("line: failed to append newline to output buffer", .{});
    };
    return 0;
}

/// rb.data() — return table with system info: os, arch, hostname, home, username.
fn data(lua: *Lua) i32 {
    lua.newTable();

    const uts = std.posix.uname();
    _ = lua.pushString(std.mem.sliceTo(&uts.sysname, 0));
    lua.setField(-2, "os");

    _ = lua.pushString(std.mem.sliceTo(&uts.machine, 0));
    lua.setField(-2, "arch");

    var hostname_buf: [std.posix.HOST_NAME_MAX]u8 = undefined;
    const hostname = std.posix.gethostname(&hostname_buf) catch "unknown";
    _ = lua.pushString(hostname);
    lua.setField(-2, "hostname");

    if (std.c.getenv("HOME")) |home| {
        _ = lua.pushString(std.mem.span(home));
        lua.setField(-2, "home");
    }

    if (std.c.getenv("USER")) |user| {
        _ = lua.pushString(std.mem.span(user));
        lua.setField(-2, "username");
    }

    return 1;
}

/// rb.ref_file(path) — track a static input file in the context.
fn refFile(lua: *Lua) i32 {
    const ctx = Context.fromLua(lua);
    const path = lua.checkString(1);

    const duped = ctx.allocator.dupe(u8, path) catch {
        lua.raiseErrorStr("ref_file: allocation failed", .{});
    };

    ctx.static_inputs.append(ctx.allocator, duped) catch {
        ctx.allocator.free(duped);
        lua.raiseErrorStr("ref_file: failed to track file", .{});
    };

    return 0;
}
