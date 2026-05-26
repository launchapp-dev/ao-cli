use clap::Args;

#[derive(Debug, Args)]
pub(crate) struct DoctorArgs {
    #[arg(long, help = "Apply safe local remediations for doctor findings.")]
    pub(crate) fix: bool,
    #[arg(
        long,
        value_name = "NAME",
        help = "Run only checks whose id contains the given substring (repeatable, case-insensitive)."
    )]
    pub(crate) filter: Vec<String>,
    #[arg(long, help = "Skip checks that spawn external subprocesses (cosign verify, plugin --manifest probes).")]
    pub(crate) skip_subprocess: bool,
}
