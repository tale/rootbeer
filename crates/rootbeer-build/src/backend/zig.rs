use super::{Context, Phase};

pub(super) fn plan(options: &[String], context: &Context<'_>) -> Result<Vec<Phase>, String> {
    let zig = context.tools.join("zig");
    if !zig.is_file() {
        return Err("Zig builds require an exact dependency providing zig".into());
    }
    let mut arguments = vec![
        zig.to_string_lossy().into_owned(),
        "build".into(),
        "--prefix".into(),
        context.prefix.to_string_lossy().into_owned(),
        "--cache-dir".into(),
        context
            .workspace
            .join("zig-cache")
            .to_string_lossy()
            .into_owned(),
        "--global-cache-dir".into(),
        context
            .workspace
            .join("zig-global-cache")
            .to_string_lossy()
            .into_owned(),
        format!("-j{}", context.jobs),
    ];
    arguments.extend_from_slice(options);
    Ok(vec![Phase {
        requires_network: false,
        name: "build",
        commands: vec![arguments],
    }])
}
