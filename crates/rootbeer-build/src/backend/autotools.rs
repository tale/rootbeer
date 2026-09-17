use super::{Context, Phase};

pub(super) fn plan(options: &[String], context: &Context<'_>) -> Vec<Phase> {
    let mut configure = vec!["sh".into(), "./configure".into(), "--prefix=/".into()];
    configure.extend(options.iter().map(|argument| context.expand(argument)));
    vec![
        Phase {
            requires_network: false,
            name: "configure",
            commands: vec![configure],
        },
        Phase {
            requires_network: false,
            name: "build",
            commands: vec![vec!["make".into(), format!("-j{}", context.jobs)]],
        },
        Phase {
            requires_network: false,
            name: "check",
            commands: vec![vec!["make".into(), "check".into()]],
        },
        Phase {
            requires_network: false,
            name: "install",
            commands: vec![vec![
                "make".into(),
                format!("DESTDIR={}", context.prefix.display()),
                "install".into(),
            ]],
        },
    ]
}
