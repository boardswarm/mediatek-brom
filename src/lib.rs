#![cfg_attr(docsrs, feature(doc_auto_cfg))]
#![doc = include_str!("../README.md")]
use std::fmt::Debug;

pub mod io;

#[derive(Debug)]
#[allow(dead_code)]
enum Command {
    SendDa = 0xd7,
    JumpDa = 0xd5,
    JumpDa64 = 0xde,
    GetHwCode = 0xfd,
    GetHwSwVer = 0xfc,
}

#[derive(thiserror::Error, Debug, Clone)]
pub enum BromError {
    #[error("Incorrect echo response")]
    IncorrectEchoResponse,
    #[error("Incorrect handshake response")]
    IncorrectHandshakeResponse,
    #[error("Unexpected status reported: {0}")]
    UnexpectedStatus(u16),
}

/// Operations to be executed by calling code to finish a request
pub trait Operation {
    type Value: Debug;
    fn step(&mut self) -> Step<'_, Self::Value>;
    fn chain<O>(self, next: O) -> Chain<Self, O>
    where
        Self: Sized,
        O: Operation,
    {
        Chain { a: self, b: next }
    }

    fn map<U, F>(self, map: F) -> Map<Self, F>
    where
        Self: Sized,
        F: Fn(Self::Value) -> U,
    {
        Map {
            operation: self,
            map,
        }
    }
}

pub struct Map<O, F> {
    operation: O,
    map: F,
}

impl<O, F, U> Operation for Map<O, F>
where
    O: Operation,
    F: Fn(O::Value) -> U,
    U: Debug,
{
    type Value = U;

    fn step(&mut self) -> Step<'_, Self::Value> {
        self.operation.step().map(&self.map)
    }
}

pub struct Chain<A, B> {
    a: A,
    b: B,
}

impl<A, B> Operation for Chain<A, B>
where
    A: Operation,
    B: Operation,
{
    type Value = B::Value;

    fn step(&mut self) -> Step<'_, Self::Value> {
        self.a.step().chain(|_| self.b.step())
    }
}

/// IO operation that should be executed
#[derive(Debug)]
pub enum Io<'a> {
    /// Read data from brom transport. Reads should fill the whole array
    ReadData(&'a mut [u8]),
    /// Write all data over the brom transport
    WriteData(&'a [u8]),
}

#[derive(Debug)]
pub enum Step<'a, T> {
    // Execute the requested IO
    Io(Io<'a>),
    /// Request is done
    Done(Result<T, BromError>),
}

impl<'a, T> Step<'a, T> {
    fn chain<U, F>(self, chain: F) -> Step<'a, U>
    where
        F: FnOnce(T) -> Step<'a, U>,
    {
        match self {
            Step::Io(io) => Step::Io(io),
            Step::Done(Err(e)) => Step::Done(Err(e)),
            Step::Done(Ok(v)) => chain(v),
        }
    }

    fn and_then<U, F>(self, op: F) -> Step<'a, U>
    where
        F: FnOnce(T) -> Result<U, BromError>,
    {
        match self {
            Step::Io(io) => Step::Io(io),
            Step::Done(Err(e)) => Step::Done(Err(e)),
            Step::Done(Ok(v)) => Step::Done(op(v)),
        }
    }

    fn map<U, F>(self, op: F) -> Step<'a, U>
    where
        F: FnOnce(T) -> U,
    {
        match self {
            Step::Io(io) => Step::Io(io),
            Step::Done(Err(e)) => Step::Done(Err(e)),
            Step::Done(Ok(v)) => Step::Done(Ok(op(v))),
        }
    }
}
const HANDSHAKE: &[u8] = &[0xa0, 0x0a, 0x50, 0x05];
#[derive(Debug)]
struct HandShake {
    offset: usize,
    data: [u8; 1],
    written: bool,
}

impl HandShake {
    fn new() -> Self {
        Self {
            offset: 0,
            data: [0; 1],
            written: false,
        }
    }
}

impl Operation for HandShake {
    type Value = ();

    fn step(&mut self) -> Step<'_, Self::Value> {
        if self.written {
            self.offset += 1;
            self.written = false;
            Step::Io(Io::ReadData(&mut self.data))
        } else {
            /* Check read data */
            if self.offset > 0 && self.data[0] != !HANDSHAKE[self.offset - 1] {
                Step::Done(Err(BromError::IncorrectHandshakeResponse))
            } else if self.offset >= HANDSHAKE.len() {
                Step::Done(Ok(()))
            } else {
                self.written = true;
                Step::Io(Io::WriteData(&HANDSHAKE[self.offset..self.offset + 1]))
            }
        }
    }
}

#[derive(Default)]
struct CheckStatus {
    status: Read<2>,
}

impl Operation for CheckStatus {
    type Value = ();

    fn step(&mut self) -> Step<'_, Self::Value> {
        self.status.step().and_then(|v| {
            if v == [0, 0] {
                Ok(())
            } else {
                Err(BromError::UnexpectedStatus(u16::from_be_bytes(v)))
            }
        })
    }
}

struct WriteData<'a> {
    data: &'a [u8],
    written: bool,
}

impl<'a> WriteData<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            written: false,
        }
    }
}

impl Operation for WriteData<'_> {
    type Value = ();

    fn step(&mut self) -> Step<'_, Self::Value> {
        if self.written {
            Step::Done(Ok(()))
        } else {
            self.written = true;
            Step::Io(Io::WriteData(self.data))
        }
    }
}

struct Read<const N: usize> {
    in_: [u8; N],
    read: bool,
}

impl<const N: usize> Default for Read<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> Read<N> {
    fn new() -> Self {
        Self {
            in_: [0; N],
            read: false,
        }
    }
}

impl<const N: usize> Operation for Read<N> {
    type Value = [u8; N];

    fn step(&mut self) -> Step<'_, Self::Value> {
        if self.read {
            Step::Done(Ok(self.in_))
        } else {
            self.read = true;
            Step::Io(Io::ReadData(&mut self.in_))
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum EchoState {
    Out,
    In,
    Done,
}
struct Echo<const N: usize> {
    out: [u8; N],
    in_: [u8; N],
    state: EchoState,
}

impl<const N: usize> Echo<N> {
    pub fn new(out: [u8; N]) -> Self {
        Self {
            out,
            in_: [0; N],
            state: EchoState::Out,
        }
    }
}

impl<const N: usize> Operation for Echo<N> {
    type Value = ();

    fn step(&mut self) -> Step<'_, Self::Value> {
        match self.state {
            EchoState::Out => {
                self.state = EchoState::In;
                Step::Io(Io::WriteData(&self.out))
            }
            EchoState::In => {
                self.state = EchoState::Done;
                Step::Io(Io::ReadData(&mut self.in_))
            }
            EchoState::Done => {
                if self.in_ == self.out {
                    Step::Done(Ok(()))
                } else {
                    Step::Done(Err(BromError::IncorrectEchoResponse))
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HwCode {
    /// Hardware code in hex
    pub code: u16,
    /// Hardware version
    pub version: u16,
}

#[derive(Debug)]
pub struct Brom {
    address: u32,
}

impl Brom {
    /// Start handshake with the bootrom; The address indicates where to
    /// load/execute the Download Agent (DA)
    pub fn handshake(address: u32) -> impl Operation<Value = Self> {
        HandShake::new().map(move |_| Self { address })
    }

    /// Get the hardware information from the bootrom
    pub fn hwcode(&self) -> impl Operation<Value = HwCode> {
        Echo::new([Command::GetHwCode as u8]).chain(Read::new().map(|v: [u8; 4]| {
            let code = u16::from_be_bytes(v[0..2].try_into().unwrap());
            let version = u16::from_be_bytes(v[2..4].try_into().unwrap());
            HwCode { code, version }
        }))
    }

    // Send DA to bootrom memory
    pub fn send_da<'d>(&self, data: &'d [u8]) -> impl Operation<Value = ()> + 'd {
        let len = data.len() as u32;
        Echo::new([Command::SendDa as u8])
            .chain(Echo::new(self.address.to_be_bytes()))
            .chain(Echo::new(len.to_be_bytes()))
            // Empty signature
            .chain(Echo::new([0; 4]))
            .chain(CheckStatus::default())
            .chain(WriteData::new(data))
            // TODO check checksum reported by brom
            .chain(Read::<2>::new())
            .chain(CheckStatus::default())
    }

    // Execute a 64 bit DA. Ensure that one has been send first!
    pub fn jump_da64(&self) -> impl Operation<Value = ()> {
        Echo::new([Command::JumpDa64 as u8])
            .chain(Echo::new(self.address.to_be_bytes()))
            .chain(Echo::new([0x1]))
            .chain(CheckStatus::default())
            .chain(Echo::new([0x64]))
            .chain(CheckStatus::default())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[derive(Debug)]
    enum ExpectedIo<'a> {
        Read(&'a [u8]),
        Write(&'a [u8]),
    }

    struct ExpectedSteps<'a>(&'a [ExpectedIo<'a>]);
    impl ExpectedSteps<'_> {
        fn validate<O: Operation>(&self, operation: &mut O) -> Result<O::Value, super::BromError> {
            for (i, expected) in self.0.iter().enumerate() {
                let op = operation.step();
                if let Step::Io(io) = op {
                    match (expected, io) {
                        (ExpectedIo::Read(exp), Io::ReadData(r)) => {
                            assert_eq!(exp.len(), r.len(), "Unexpected read length; step {i}");
                            r.copy_from_slice(exp);
                        }
                        (ExpectedIo::Write(exp), Io::WriteData(r)) => {
                            assert_eq!(exp, &r, "Mismatched write; step {i}");
                        }
                        (expected, got) => panic!(
                            "Mismatched operation {i}: expected {:?}  got: {:?}",
                            expected, got
                        ),
                    }
                } else {
                    panic!("Unexpectedly done: {:?}", op);
                }
            }

            let step = operation.step();
            if let Step::Done(r) = step {
                r
            } else {
                panic!("Expected to be done, but got: {:?}", step);
            }
        }
    }

    const EXPECTED_HANDSHAKE: ExpectedSteps = ExpectedSteps(&[
        ExpectedIo::Write(&[0xa0]),
        ExpectedIo::Read(&[!0xa0]),
        ExpectedIo::Write(&[0x0a]),
        ExpectedIo::Read(&[!0x0a]),
        ExpectedIo::Write(&[0x50]),
        ExpectedIo::Read(&[!0x50]),
        ExpectedIo::Write(&[0x05]),
        ExpectedIo::Read(&[!0x05]),
    ]);

    #[test]
    fn handshake() {
        let mut handshake = Brom::handshake(0x1234);
        EXPECTED_HANDSHAKE.validate(&mut handshake).unwrap();
    }

    #[test]
    fn hwcode() {
        const HWCODE: ExpectedSteps = ExpectedSteps(&[
            ExpectedIo::Write(&[0xfd]),
            ExpectedIo::Read(&[0xfd]),
            ExpectedIo::Read(&[0x81, 0x88, 0x2, 0x3]),
        ]);
        let mut handshake = Brom::handshake(0x1234);
        let p = EXPECTED_HANDSHAKE.validate(&mut handshake).unwrap();
        let hwcode = HWCODE.validate(&mut p.hwcode()).unwrap();
        assert_eq!(
            hwcode,
            HwCode {
                code: 0x8188,
                version: 0x0203
            }
        );
    }

    #[test]
    fn send_da() {
        const DATA: &[u8] = &[0x1, 0x2, 0x3, 0x4];
        const SEND_DA: ExpectedSteps = ExpectedSteps(&[
            // cmd
            ExpectedIo::Write(&[0xd7]),
            ExpectedIo::Read(&[0xd7]),
            // address
            ExpectedIo::Write(&[0x00, 0x00, 0x12, 0x34]),
            ExpectedIo::Read(&[0x00, 0x00, 0x12, 0x34]),
            // length
            ExpectedIo::Write(&[0x00, 0x00, 0x00, 0x04]),
            ExpectedIo::Read(&[0x00, 0x00, 0x00, 0x04]),
            // (no) signature
            ExpectedIo::Write(&[0x00, 0x00, 0x00, 0x00]),
            ExpectedIo::Read(&[0x00, 0x00, 0x00, 0x00]),
            // status
            ExpectedIo::Read(&[0x00, 0x00]),
            // data
            ExpectedIo::Write(DATA),
            // checksum; TODO calculate
            ExpectedIo::Read(&[0x0, 0x0]),
            // status
            ExpectedIo::Read(&[0x0, 0x0]),
        ]);
        let mut handshake = Brom::handshake(0x1234);
        let p = EXPECTED_HANDSHAKE.validate(&mut handshake).unwrap();
        SEND_DA.validate(&mut p.send_da(DATA)).unwrap();
    }

    #[test]
    fn jump_da64() {
        const JUMP_DA64: ExpectedSteps = ExpectedSteps(&[
            // cmd
            ExpectedIo::Write(&[0xde]),
            ExpectedIo::Read(&[0xde]),
            // address
            ExpectedIo::Write(&[0x00, 0x00, 0x12, 0x34]),
            ExpectedIo::Read(&[0x00, 0x00, 0x12, 0x34]),
            // Confirm?
            ExpectedIo::Write(&[0x01]),
            ExpectedIo::Read(&[0x01]),
            // status
            ExpectedIo::Read(&[0x00, 0x00]),
            // execute 64 bit?
            ExpectedIo::Write(&[0x64]),
            ExpectedIo::Read(&[0x64]),
            // status
            ExpectedIo::Read(&[0x00, 0x00]),
        ]);
        let mut handshake = Brom::handshake(0x1234);
        let p = EXPECTED_HANDSHAKE.validate(&mut handshake).unwrap();
        JUMP_DA64.validate(&mut p.jump_da64()).unwrap();
    }
}
