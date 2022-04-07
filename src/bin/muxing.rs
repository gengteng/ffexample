use clap::Parser;
use std::path::PathBuf;

#[derive(Debug, clap::Parser)]
struct Opts {
    #[clap()]
    destination: PathBuf,
}

struct Muxing {}

impl Muxing {
    pub fn new(_opts: Opts) -> anyhow::Result<Self> {
        Ok(Self {})
    }

    pub fn run(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    Muxing::new(opts)?.run()
}
