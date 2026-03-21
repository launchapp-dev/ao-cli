use clap::Args;

#[derive(Debug, Args)]
pub(crate) struct DoctorArgs {
    #[arg(long, help = "Apply safe local remediations for doctor findings.")]
    pub(crate) fix: bool,
    #[arg(
        long,
        value_name = "CATEGORY",
        help = "Run only diagnostics for the specified category. Available: runner, daemon, config, network, api-keys"
    )]
    pub(crate) check: Option<String>,
}
