use super::{Context, Phase};
use rootbeer_package::BuildSteps;

pub(super) fn plan(
    steps: Option<&BuildSteps>,
    context: &Context<'_>,
) -> Result<Vec<Phase>, String> {
    let steps = steps.ok_or("custom builds require explicit phases")?;
    Ok([
        ("configure", &steps.configure),
        ("build", &steps.build),
        ("check", &steps.check),
        ("install", &steps.install),
    ]
    .into_iter()
    .map(|(name, commands)| Phase {
        requires_network: false,
        name,
        commands: commands
            .iter()
            .map(|command| {
                command
                    .iter()
                    .map(|argument| context.expand(argument))
                    .collect()
            })
            .collect(),
    })
    .collect())
}
