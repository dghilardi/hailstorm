use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    tonic_build::configure().compile_protos(&["hailstorm.proto"], &["proto"])?;

    Ok(())
}
