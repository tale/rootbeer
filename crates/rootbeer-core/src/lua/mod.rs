mod fs;
mod require;
mod serializer;
mod sys;
mod vm;

use crate::Runtime;
use mlua::{AppDataRef, Lua};
pub(crate) use vm::{create_vm, Run};

/// Extract the shared Runtime and Run from Lua app data.
pub(super) fn ctx(lua: &Lua) -> (AppDataRef<'_, Runtime>, AppDataRef<'_, Run>) {
    (
        lua.app_data_ref::<Runtime>().expect("Runtime not set"),
        lua.app_data_ref::<Run>().expect("Run not set"),
    )
}
