use std::io;

use anyhow::Result;
use clap::{Args, Command};
use clap_complete::{
    generate,
    shells::{Bash, Elvish, Fish, PowerShell, Shell, Zsh},
};

#[derive(Args)]
pub struct Arg {
    /// Type of shell
    #[clap(arg_enum)]
    shell: Shell,
}

impl Arg {
    pub async fn handle(&self, app: &mut Command<'_>) -> Result<()> {
        // NOTE: We should be able to use `self.shell.generate`
        // if bin_name can be automatically propagated to subcommands
        const BIN_NAME: &str = "rkubectl";
        let buf = &mut io::stdout();
        match self.shell {
            Shell::Bash => generate(Bash, app, BIN_NAME, buf),
            Shell::Elvish => generate(Elvish, app, BIN_NAME, buf),
            Shell::Fish => generate(Fish, app, BIN_NAME, buf),
            Shell::PowerShell => generate(PowerShell, app, BIN_NAME, buf),
            Shell::Zsh => generate(Zsh, app, BIN_NAME, buf),
            _ => println!("Unsupported shell"),
        }
        Ok(())
    }
}
