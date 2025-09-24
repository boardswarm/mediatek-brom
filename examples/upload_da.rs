use anyhow::anyhow;
use clap::Parser;
use clap_num::maybe_hex;
use mediatek_brom::{io::BromExecuteAsync, Brom};
use std::path::PathBuf;
use tokio_serial::SerialPortBuilderExt;

#[derive(Debug, clap::Parser)]
struct Opts {
    #[clap(short, long, default_value = "115200")]
    rate: u32,
    #[clap(short, long, value_parser = maybe_hex::<u32>)]
    address: Option<u32>,
    path: String,
    da: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let mut serial = tokio_serial::new(&opts.path, opts.rate).open_native_async()?;

    let brom = serial.execute(Brom::handshake()).await?;

    let hwcode = serial.execute(brom.hwcode()).await?;
    println!("Hwcode: {:x?}", hwcode);

    let address = opts
        .address
        .or(hwcode.da_address())
        .ok_or(anyhow!("Failed to determine DA address"))?;

    let data = tokio::fs::read(&opts.da).await?;
    println!("Uploading DA to {:#x}", address);
    serial.execute(brom.send_da(address, &data)).await?;
    println!("Executing DA");
    serial.execute(brom.jump_da64(address)).await?;

    Ok(())
}
