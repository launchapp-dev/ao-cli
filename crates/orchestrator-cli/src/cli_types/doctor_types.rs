use clap::Args;

#[derive(Debug, Args)]
pub(crate) struct DoctorArgs {
    #[arg(long, help = "Apply safe local remediations for doctor findings.")]
    pub(crate) fix: bool,
}
