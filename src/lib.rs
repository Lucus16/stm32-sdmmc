#![no_std]
#[cfg(any(feature = "stm32l4x1", feature = "stm32l4x6"))]
mod stm32l4xx;
#[cfg(any(feature = "stm32l4x1", feature = "stm32l4x6"))]
pub use stm32l4xx::{Config, Device, Pins};

pub const BLOCK_SIZE: usize = 0x200;

/// The Block type wraps a byte array with the size of one block and the alignment necessary for
/// reading and writing it.
#[repr(C, align(4))]
#[derive(Clone, Copy)]
pub struct Block(pub [u8; BLOCK_SIZE]);

pub type BlockCount = u32;
pub type BlockIndex = u32;

impl<I: core::slice::SliceIndex<[u8]>> core::ops::Index<I> for Block {
    type Output = I::Output;
    #[inline]
    fn index(&self, index: I) -> &I::Output {
        &self.0[index]
    }
}

impl<I: core::slice::SliceIndex<[u8]>> core::ops::IndexMut<I> for Block {
    #[inline]
    fn index_mut(&mut self, index: I) -> &mut I::Output {
        &mut self.0[index]
    }
}

impl Block {
    pub fn zeroed() -> Self {
        Block([0; BLOCK_SIZE])
    }
}

#[derive(Copy, Clone, Debug)]
pub enum Error {
    /// Card does not respond at all, it is probably missing or unpowered.
    NoCard,
    /// The card host has not yet been initialized, call .init() first.
    Uninitialized,
    /// The DMA peripheral could not keep up with the card during a read. Adjust the relative clock
    /// speeds.
    ReceiveOverrun,
    /// The DMA peripheral could not keep up with the card during a write. Adjust the relative
    /// clock speeds.
    SendUnderrun,
    /// A command or IO operation timed out. Try to reinitialize by calling .init() again.
    Timeout,
    /// A CRC check failed for a command or IO operation. Retry the operation.
    CRCFail,
    /// The card does not support the supplied voltage.
    OperatingConditionsNotSupported,
    /// The card gave an unexpected response.
    UnexpectedResponse,
    /// The operation did not succeed and the card host did not indicate why.
    UnknownResult,
    /// An operation is still running. Call .result() until it no longer return WouldBlock.
    Busy,
    /// A result was requested but no operation was started.
    NoOperation,
    /// The parsed value was not valid.
    InvalidValue,
}

#[derive(Copy, Clone, Debug)]
pub enum CardVersion {
    V1SC,
    V2SC,
    V2HC,
}

#[derive(Copy, Clone, Debug)]
#[allow(non_camel_case_types)]
pub enum Command {
    GO_IDLE_STATE = 0,
    ALL_SEND_CID = 2,
    SEND_RELATIVE_ADDR = 3,
    SELECT_CARD = 7,
    SEND_IF_COND = 8,
    SEND_CSD = 9,
    SEND_CID = 10,
    SEND_STATUS = 13,
    READ_BLOCK = 17,
    READ_MULTIPLE_BLOCK = 18,
    SET_BLOCK_COUNT = 23,
    WRITE_BLOCK = 24,
    WRITE_MULTIPLE_BLOCK = 25,
    ERASE_WR_BLK_START = 32,
    ERASE_WR_BLK_END = 33,
    ERASE = 38,
    APP_COMMAND = 55,
}

#[derive(Copy, Clone, Debug)]
#[allow(non_camel_case_types)]
pub enum AppCommand {
    SET_BUS_WIDTH = 6,
    SD_STATUS = 13,
    SET_WR_BLK_ERASE_COUNT = 23,
    SD_SEND_OP_COND = 41,
}

#[derive(Copy, Clone, Debug)]
pub enum BusWidth {
    Bits1,
    Bits4,
}

#[derive(Copy, Clone, Debug)]
enum CSD {
    V1([u32; 4]),
    V2([u32; 4]),
}

#[derive(Copy, Clone, Debug)]
pub enum CardState {
    Idle = 0,
    Ready = 1,
    Ident = 2,
    Standby = 3,
    Transmit = 4,
    Data = 5,
    Receive = 6,
    Program = 7,
    Disabled = 8,
    Reserved,
}

pub type CID = [u32; 4];

#[repr(C, align(4))]
pub struct SDStatus([u8; 64]);

pub struct CardStatus(u32);

const ERROR_MASK: u32 = 0xfff98004;

impl CardStatus {
    pub fn any_error(&self) -> bool {
        self.0 & ERROR_MASK != 0
    }

    pub fn ready_for_data(&self) -> bool {
        (self.0 >> 8) & 1 != 0
    }

    pub fn app_cmd(&self) -> bool {
        (self.0 >> 5) & 1 != 0
    }

    pub fn state(&self) -> CardState {
        use CardState::*;
        match (self.0 >> 9) & 0xf {
            0 => Idle,
            1 => Ready,
            2 => Ident,
            3 => Standby,
            4 => Transmit,
            5 => Data,
            6 => Receive,
            7 => Program,
            8 => Disabled,
            _ => Reserved,
        }
    }
}

impl core::fmt::Debug for SDStatus {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "SDStatus(")?;
        write!(f, "au_size={:x?}, ", self.au_size())?;
        write!(f, "data_bus_width={:?}, ", self.data_bus_width())?;
        write!(f, "discard_support={:?}, ", self.discard_support())?;
        write!(f, "erase_size={:x?}, ", self.erase_size())?;
        write!(f, "erase_timeout={:?}s, ", self.erase_timeout())?;
        write!(f, "fule_support={:?}, ", self.fule_support())?;
        write!(f, "sd_card_type={:?})", self.sd_card_type())?;
        Ok(())
    }
}

impl SDStatus {
    pub fn data_bus_width(&self) -> Result<BusWidth, Error> {
        match self.0[0x00] >> 6 {
            0 => Ok(BusWidth::Bits1),
            2 => Ok(BusWidth::Bits4),
            _ => Err(Error::InvalidValue),
        }
    }

    /// The size of an allocation unit in bytes.
    pub fn au_size(&self) -> Result<usize, Error> {
        Ok(match self.0[0x0a] >> 4 {
            0x1 => 16 * 1024,
            0x2 => 32 * 1024,
            0x3 => 64 * 1024,
            0x4 => 128 * 1024,
            0x5 => 256 * 1024,
            0x6 => 512 * 1024,
            0x7 => 1024 * 1024,
            0x8 => 2 * 1024 * 1024,
            0x9 => 4 * 1024 * 1024,
            0xa => 8 * 1024 * 1024,
            0xb => 12 * 1024 * 1024,
            0xc => 16 * 1024 * 1024,
            0xd => 24 * 1024 * 1024,
            0xe => 32 * 1024 * 1024,
            0xf => 64 * 1024 * 1024,
            _ => return Err(Error::InvalidValue),
        })
    }

    pub fn sd_card_type(&self) -> usize {
        self.0[0x03] as usize | ((self.0[0x02] as usize) << 8)
    }

    /// The number of allocation units to be erased at a time.
    pub fn erase_size(&self) -> usize {
        self.0[0x0c] as usize | ((self.0[0x0b] as usize) << 8)
    }

    /// The number of seconds it takes to erase a single erase area.
    pub fn erase_timeout(&self) -> usize {
        (self.0[0x0d] as usize) >> 2
    }

    /// SD card supports discard.
    pub fn discard_support(&self) -> bool {
        (self.0[0x18] >> 1) & 1 != 0
    }

    /// SD card supports Full User area Logical Erase. erase_card takes at most one second.
    pub fn fule_support(&self) -> bool {
        self.0[0x18] & 1 != 0
    }
}

impl CSD {
    fn capacity(&self) -> BlockCount {
        match self {
            CSD::V1(words) => {
                let c_size = (words[2] >> 30 | (words[1] & 0x3ff) << 2) + 1;
                let c_size_mult = ((words[2] & 0x0003_8000) >> 15) + 2;
                c_size << c_size_mult
            }
            CSD::V2(words) => {
                let c_size = (words[2] >> 16) + 1;
                c_size << 10
            }
        }
    }
}

pub trait CardHost {
    /// Initialize the SD card.
    fn init_card(&mut self) -> nb::Result<(), Error>;

    /// Return the card identification number.
    fn card_id(&mut self) -> Result<CID, Error>;

    /// Return the card size in blocks.
    fn card_size(&mut self) -> Result<BlockCount, Error>;

    /// Erase the entire card.
    fn erase_card(&mut self) -> Result<(), Error>;

    /// Read the SD Status register.
    fn read_sd_status(&mut self) -> Result<SDStatus, Error>;

    /// Erase blocks on the SD card.
    fn erase(&mut self, start: BlockIndex, end: BlockIndex) -> Result<(), Error>;

    /// Reset the card host, disabling it until the next initialization.
    fn reset(&mut self);

    /// Read a block from the SD card into memory. This function is unsafe because it writes to the
    /// passed memory block after the end of its lifetime. Make sure to keep it around and avoid
    /// reading or writing to it until the operation is finished.
    unsafe fn read_block(&mut self, block: &mut Block, address: BlockIndex) -> Result<(), Error>;

    /// Write multiple blocks from the SD card into memory. This function is unsafe because it
    /// reads from the passed memory blocks after the end of their lifetime. Make sure to keep them
    /// around and avoid writing to them until the operation is finished.
    unsafe fn write_blocks(&mut self, blocks: &[Block], address: BlockIndex) -> Result<(), Error>;

    /// Write a block from the SD card into memory. This function is unsafe because it reads from the
    /// passed memory block after the end of its lifetime. Make sure to keep it around and avoid
    /// writing to it until the operation is finished.
    unsafe fn write_block(&mut self, block: &Block, address: BlockIndex) -> Result<(), Error> {
        self.write_blocks(core::slice::from_ref(block), address)
    }

    /// Check the result of a read or write operation.
    fn result(&mut self) -> nb::Result<(), Error>;
}
