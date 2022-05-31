use std::vec::Vec;

use anyhow::{anyhow, Context, Result};
use clap::Args;
use reqwest::blocking::Client;

#[derive(Args)]
pub struct Arg {
    /// Name of Workflow
    workflow: String,
}

impl Arg {
    pub fn handle(&self) -> Result<()> {
        Ok(())
    }
}
