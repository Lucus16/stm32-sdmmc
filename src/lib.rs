#[cfg(feature = "stm32l4x6")]
pub mod stm32l4x6;

pub type Block = [u8; 512];
pub struct BlockCount(u32);
pub struct BlockIndex(u32);

#[derive(Copy, Clone, Debug)]
pub enum Error {
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
}

#[derive(Copy, Clone, Debug)]
pub enum CardVersion {
    V1,
    V2,
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
    WRITE_BLOCK = 24,
    APP_COMMAND = 55,
}

#[derive(Copy, Clone, Debug)]
#[allow(non_camel_case_types)]
pub enum AppCommand {
    SD_SEND_OP_COND = 41,
}

pub trait CardHost {
    /// Initialize the SD card and the DMA channel.
    fn init(&mut self) -> Result<(), Error>;

    /// Return the card size in blocks.
    fn card_size(&mut self) -> Result<BlockCount, Error>;

    /// Read a block from the SD card into memory. This function is unsafe because it writes to the
    /// passed memory block after the end of its lifetime. Make sure to keep it around and avoid
    /// reading or writing to it until the operation is finished.
    unsafe fn read_block(&mut self, block: &mut Block, address: BlockIndex) -> nb::Result<(), Error>;

    /// Write a block from the SD card into memory. This function is unsafe because it reads from the
    /// passed memory block after the end of its lifetime. Make sure to keep it around and avoid
    /// writing to it until the operation is finished.
    unsafe fn write_block(&mut self, block: &Block, address: BlockIndex) -> nb::Result<(), Error>;

    /// Check the result of a read or write operation.
    fn result(&mut self) -> nb::Result<(), Error>;
}
