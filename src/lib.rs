#[cfg(feature = "stm32l4x6")]
pub mod stm32l4x6;

pub type Block = [u8; 512];
pub struct BlockCount(u32);
pub struct BlockIndex(u32);

#[derive(Copy, Clone, Debug)]
pub enum Error {
    ReceiveOverrun,
    SendUnderrun,
    Timeout,
    CRCFail,
    OperatingConditionsNotSupported,
    ResponseToOtherCommand,
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
    APP_COMMAND = 55,
}

#[derive(Copy, Clone, Debug)]
#[allow(non_camel_case_types)]
pub enum AppCommand {
    SD_SEND_OP_COND = 41,
}

pub trait CardHost {
    fn init(&self) -> Result<(), Error>;
    unsafe fn read_block(&mut self, block: &mut Block, address: BlockIndex) -> Result<(), Error>;
    unsafe fn write_block(&mut self, block: &Block, address: BlockIndex) -> Result<(), Error>;
}
