use skyrim_alchemy_rs::do_the_thing;

fn main() -> Result<(), anyhow::Error> {
    env_logger::init();

    do_the_thing()?;
    Ok(())
}
