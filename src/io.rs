use std::{
    future::Future,
    io::{Read, Write},
};

use thiserror::Error;

use crate::{BromError, Io, Operation, Step};

pub trait BromExecute<E>
where
    E: From<BromError>,
{
    fn io(&mut self, op: Io<'_>) -> Result<(), E>;
    fn execute<O>(&mut self, mut o: O) -> Result<O::Value, E>
    where
        O: Operation,
    {
        loop {
            match o.step() {
                Step::Io(op) => self.io(op)?,
                Step::Done(d) => return Ok(d?),
            }
        }
    }
}

#[derive(Error, Debug)]
pub enum IOError {
    #[error("I/O error: {0}")]
    IO(#[from] std::io::Error),
    #[error("Bootrom error: {0}")]
    Brom(#[from] BromError),
}

impl<IO> BromExecute<IOError> for IO
where
    IO: Read + Write,
{
    fn io(&mut self, op: Io<'_>) -> Result<(), IOError> {
        match op {
            Io::ReadData(r) => self.read_exact(r)?,
            Io::WriteData(w) => self.write_all(w)?,
        }
        Ok(())
    }
}

pub trait BromExecuteAsync<E>
where
    E: From<BromError>,
{
    fn io(&mut self, op: Io<'_>) -> impl Future<Output = Result<(), E>> + Send;
    fn execute<O>(&mut self, mut o: O) -> impl Future<Output = Result<O::Value, E>> + Send
    where
        O: Operation + Send,
        O::Value: Send,
        Self: Send,
    {
        async move {
            loop {
                match o.step() {
                    Step::Io(op) => self.io(op).await?,
                    Step::Done(d) => return Ok(d?),
                }
            }
        }
    }
}

#[cfg(feature = "tokio")]
impl<IO> BromExecuteAsync<IOError> for IO
where
    IO: tokio::io::AsyncWriteExt,
    IO: tokio::io::AsyncReadExt,
    IO: Unpin + Send,
{
    async fn io(&mut self, op: Io<'_>) -> Result<(), IOError> {
        match op {
            Io::ReadData(r) => {
                self.read_exact(r).await?;
            }
            Io::WriteData(w) => self.write_all(w).await?,
        }
        Ok(())
    }
}

#[cfg(all(feature = "futures", not(feature = "tokio")))]
impl<IO> BromExecuteAsync<IOError> for IO
where
    IO: futures::AsyncReadExt,
    IO: futures::AsyncWriteExt,
    IO: Unpin + Send,
{
    async fn io(&mut self, op: Io<'_>) -> Result<(), IOError> {
        match op {
            Io::ReadData(r) => {
                self.read_exact(r).await?;
            }
            Io::WriteData(w) => self.write_all(w).await?,
        }
        Ok(())
    }
}
