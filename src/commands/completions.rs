use clap::CommandFactory;
use clap_complete::{Shell, generate};

pub fn run(shell: Shell) {
    let mut cmd = crate::cli::Cli::command();
    generate(shell, &mut cmd, "calx", &mut std::io::stdout());
}
