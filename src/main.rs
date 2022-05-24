use rayon::ThreadPoolBuilder;
use skyrim_alchemy_rs::do_the_thing;

fn main() -> Result<(), anyhow::Error> {
    ThreadPoolBuilder::new()
        .num_threads(4)
        .build_global()
        .unwrap();
    do_the_thing()?;
    Ok(())
}
