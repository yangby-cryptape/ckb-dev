use ckb_dev::{prelude::*, Args, Config};

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let cfg = Config::load_from_files()?;
    let args = Args::load_from_inputs()?;
    args.execute(&cfg)?;
    Ok(())
}
