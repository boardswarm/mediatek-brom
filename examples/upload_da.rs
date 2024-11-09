use clap::Parser;
use clap_num::maybe_hex;
use mediatek_brom::{io::BromExecuteAsync, Brom};
use std::path::PathBuf;
use tokio_serial::SerialPortBuilderExt;

#[derive(Debug, clap::Parser)]
struct Opts {
    #[clap(short, long, default_value = "115200")]
    rate: u32,
    #[clap(short, long, value_parser = maybe_hex::<u32>, default_value = "0x201000")]
    address: u32,
    path: String,
    da: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let mut serial = tokio_serial::new(&opts.path, opts.rate).open_native_async()?;

    let brom = serial.execute(Brom::handshake(opts.address)).await?;

    let hwcode = serial.execute(brom.hwcode()).await?;
    println!("Hwcode: {:x?}", hwcode);

    let data = tokio::fs::read(&opts.da).await?;
    println!("Uploading DA to {}", opts.address);
    serial.execute(brom.send_da(&data)).await?;
    println!("Executing DA");
    serial.execute(brom.jump_da64()).await?;

    Ok(())
}
