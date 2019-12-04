#![no_std]
#[cfg(feature = "stm32l4x6")]
mod stm32l4x6;
#[cfg(feature = "stm32l4x6")]
pub use stm32l4x6::{Config, Device, Pins};

pub const BLOCK_SIZE: usize = 0x200;

pub type Block = [u8; BLOCK_SIZE];

pub type BlockCount = u32;

#[derive(Copy, Clone, Debug)]
pub struct BlockIndex(u32);

impl BlockIndex {
    pub fn new(index: u32) -> BlockIndex {
        BlockIndex(index)
    }

    pub fn as_u32(self) -> u32 {
        self.0
    }
}

impl core::ops::Add<BlockCount> for BlockIndex {
    type Output = BlockIndex;
    fn add(self, other: BlockCount) -> BlockIndex {
        BlockIndex(self.0 + other)
    }
}

impl core::ops::Add<BlockIndex> for BlockCount {
    type Output = BlockIndex;
    fn add(self, other: BlockIndex) -> BlockIndex {
        BlockIndex(self + other.0)
    }
}

impl core::ops::AddAssign<BlockCount> for BlockIndex {
    fn add_assign(&mut self, other: BlockCount) {
        self.0 += other
    }
}

impl core::ops::Sub<BlockIndex> for BlockIndex {
    type Output = BlockCount;
    fn sub(self, other: BlockIndex) -> BlockCount {
        self.0 - other.0
    }
}

impl core::ops::Sub<BlockCount> for BlockIndex {
    type Output = BlockIndex;
    fn sub(self, other: BlockCount) -> BlockIndex {
        BlockIndex(self.0 - other)
    }
}

impl core::ops::Sub<BlockIndex> for BlockCount {
    type Output = BlockIndex;
    fn sub(self, other: BlockIndex) -> BlockIndex {
        BlockIndex(self - other.0)
    }
}

impl core::ops::SubAssign<BlockCount> for BlockIndex {
    fn sub_assign(&mut self, other: BlockCount) {
        self.0 -= other
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
    READ_BLOCK = 17,
    READ_MULTIPLE_BLOCK = 18,
    SET_BLOCK_COUNT = 23,
    WRITE_BLOCK = 24,
    WRITE_MULTIPLE_BLOCK = 25,
    APP_COMMAND = 55,
}

#[derive(Copy, Clone, Debug)]
#[allow(non_camel_case_types)]
pub enum AppCommand {
    SET_BUS_WIDTH = 6,
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

    /// Return the card size in blocks.
    fn card_size(&mut self) -> Result<BlockCount, Error>;

    /// Read a block from the SD card into memory. This function is unsafe because it writes to the
    /// passed memory block after the end of its lifetime. Make sure to keep it around and avoid
    /// reading or writing to it until the operation is finished.
    unsafe fn read_block(&mut self, block: &mut Block, address: BlockIndex) -> Result<(), Error>;

    /// Write a block from the SD card into memory. This function is unsafe because it reads from the
    /// passed memory block after the end of its lifetime. Make sure to keep it around and avoid
    /// writing to it until the operation is finished.
    unsafe fn write_blocks(&mut self, blocks: &[Block], address: BlockIndex) -> Result<(), Error>;

    /// Check the result of a read or write operation.
    fn result(&mut self) -> nb::Result<(), Error>;
}
