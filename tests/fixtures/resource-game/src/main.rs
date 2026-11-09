fn main() -> engine_resources::Result<()> {
    let resources = engine_resources::Resources::discover(concat!(env!("CARGO_MANIFEST_DIR"), "/assets"))?;
    assert_eq!(resources.load("greeting")?, b"Dogwood portable resources\n");
    println!("resource fixture OK");
    Ok(())
}
