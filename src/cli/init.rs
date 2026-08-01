use anyhow::Result;

pub(crate) fn run_init(force: bool) -> Result<()> {
    match crate::config::init_config(force)? {
        crate::config::InitOutcome::Created(path) => {
            println!("Created starter config at {}", path.display());
            println!("Edit it to reserve startup commands, panel layout, theme, and more.");
        }
        crate::config::InitOutcome::AlreadyExists(path) => {
            println!(
                "Config already exists at {} — left untouched (pass --force to overwrite).",
                path.display()
            );
        }
    }
    Ok(())
}
