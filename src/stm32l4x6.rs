use stm32l4::stm32l4x6 as stm32;

use crate::{AppCommand, Block, BlockCount, BlockIndex, CardHost, CardVersion, Command, Error};
use crate::Error::*;
use nb::Error::{Other, WouldBlock};
use nb::block;

const SDMMC1_ADDRESS: u32 = 0x4001_2800;
const FIFO_OFFSET: u32 = 0x80;
const SEND_IF_COND_PATTERN: u32 = 0x0000_01aa;
const STATUS_ERROR_MASK: u32 = 0x0000_05ff;

#[derive(Copy, Clone, Debug)]
enum State {
    Uninitialized,
    Ready,
    Reading,
    Writing,
}

pub struct Device {
    sdmmc: stm32::SDMMC1,
    dma: stm32::DMA2,
    state: State,
}

impl Device {
    pub fn new(sdmmc: stm32::SDMMC1, dma: stm32::DMA2) -> Device {
        Device {
            sdmmc: sdmmc,
            dma: dma,
            state: State::Uninitialized,
        }
    }

    fn check_operating_conditions(&mut self) -> Result<(), Error> {
        match self.card_command_short(Command::SEND_IF_COND, SEND_IF_COND_PATTERN) {
            Err(e) => Err(e),
            Ok(received_pattern) => {
                if received_pattern != SEND_IF_COND_PATTERN {
                    Err(OperatingConditionsNotSupported)
                } else {
                    Ok(())
                }
            }
        }
    }

    fn acmd41(&mut self) -> Result<u32, Error> {
        self.card_command_short(Command::APP_COMMAND, 0)?;
        self.sdmmc.arg.write(|w| unsafe { w.bits(0x4010_0000) });
        self.sdmmc.cmd.write(|w| unsafe {
            w.cmdindex()
                .bits(AppCommand::SD_SEND_OP_COND as u8)
                .waitresp()
                .bits(1)
                .cpsmen()
                .set_bit()
        });

        // acmd41 does not set crc so we expect crcfail
        match block!(self.check_command(true)) {
            Err(CRCFail) => Ok(()),
            x => x,
        }?;

        Ok(self.sdmmc.resp1.read().bits())
    }

    fn card_command_none(&mut self, cmd: Command, arg: u32) -> Result<(), Error> {
        self.sdmmc.arg.write(|w| unsafe { w.bits(arg) });
        self.sdmmc.cmd.write(|w| unsafe {
            w.cmdindex()
                .bits(cmd as u8)
                .waitresp()
                .bits(0)
                .cpsmen()
                .set_bit()
        });

        block!(self.check_command(false))
    }

    fn card_command_short(&mut self, cmd: Command, arg: u32) -> Result<u32, Error> {
        self.sdmmc.arg.write(|w| unsafe { w.bits(arg) });
        self.sdmmc.cmd.write(|w| unsafe {
            w.cmdindex()
                .bits(cmd as u8)
                .waitresp()
                .bits(1)
                .cpsmen()
                .set_bit()
        });

        block!(self.check_command(true))?;
        if self.sdmmc.respcmd.read().respcmd().bits() != cmd as u8 {
            return Err(UnexpectedResponse);
        }

        Ok(self.sdmmc.resp1.read().bits())
    }

    fn card_command_long(&mut self, cmd: Command, arg: u32) -> Result<[u32; 4], Error> {
        self.sdmmc.arg.write(|w| unsafe { w.bits(arg) });
        self.sdmmc.cmd.write(|w| unsafe {
            w.cmdindex()
                .bits(cmd as u8)
                .waitresp()
                .bits(3)
                .cpsmen()
                .set_bit()
        });

        block!(self.check_command(true))?;
        // This delay helps with command recognition in the logic analyzer.
        // TODO: Remove
        let foo = 0u32;
        for _ in 0..0x200 { unsafe { core::ptr::read_volatile(&foo); } }

        Ok([
            self.sdmmc.resp1.read().bits(),
            self.sdmmc.resp2.read().bits(),
            self.sdmmc.resp3.read().bits(),
            self.sdmmc.resp4.read().bits(),
        ])
    }

    fn check_ready(&mut self) -> nb::Result<(), Error> {
        match self.state {
            State::Uninitialized => Err(Other(Error::Uninitialized)),
            State::Ready => Ok(()),
            State::Reading | State::Writing => Err(WouldBlock),
        }
    }

    fn check_command(&mut self, expect_response: bool) -> nb::Result<(), Error> {
        let status = self.sdmmc.sta.read();
        if status.cmdact().bit() {
            return Err(WouldBlock);
        }
        self.sdmmc.icr.write(|w| unsafe { w.bits(STATUS_ERROR_MASK) });
        if status.ccrcfail().bit() {
            Err(Other(CRCFail))
        } else if status.ctimeout().bit() {
            Err(Other(Timeout))
        } else if expect_response && !status.cmdrend().bit() {
            Err(Other(UnknownResult))
        } else if !expect_response && !status.cmdsent().bit() {
            Err(Other(UnknownResult))
        } else {
            Ok(())
        }
    }
}

impl CardHost for Device {
    fn init(&mut self) -> Result<(), Error> {
        // Enable power, then clock.
        self.sdmmc.clkcr.modify(|_, w| unsafe { w.clkdiv().bits(0x7e).clken().clear_bit() });
        self.sdmmc.power.modify(|_, w| unsafe { w.pwrctrl().bits(3) });
        self.sdmmc.clkcr.modify(|_, w| w.clken().set_bit());
        // TODO: Test enabling PWRSAV here.

        // Select sdmmc for dma 2 channel 4.
        self.dma.cselr.modify(|_, w| w.c4s().bits(0x7));

        // * -> idle
        self.card_command_none(Command::GO_IDLE_STATE, 0)?;

        // Determine card version.
        let _version = match self.check_operating_conditions() {
            Err(Timeout) => CardVersion::V1,
            Ok(_) => CardVersion::V2,
            e => return e
        };

        // idle -> ready
        let mut busy = true;
        while busy {
            busy = self.acmd41()? >> 31 == 0;
        }
        // ready -> ident
        self.card_command_long(Command::ALL_SEND_CID, 0)?;
        // ident -> stby
        let card_rca_status = self.card_command_short(Command::SEND_RELATIVE_ADDR, 0)?;
        let relative_card_address = card_rca_status >> 0x10;
        // stby -> tran
        self.card_command_short(Command::SELECT_CARD, relative_card_address << 16)?;

        self.state = State::Ready;

        Ok(())
    }

    fn card_size(&mut self) -> Result<BlockCount, Error> {
        panic!("not implemented: card_size");
    }

    #[allow(unused_unsafe)]
    unsafe fn read_block(&mut self, block: &mut Block, address: BlockIndex) -> nb::Result<(), Error> {
        self.check_ready()?;

        // a. Set the data length register.
        self.sdmmc
            .dlen
            .write(|w| unsafe { w.bits(block.len() as u32 as u32) });
        // Set the data timeout.
        self.sdmmc.dtimer.write(|w| unsafe { w.bits(0x2000) });

        // b. Set the dma channel.
        //    - Set the channel source address.
        self.dma
            .cmar4
            .write(|w| unsafe { w.bits(block as *const Block as u32) });
        //    - Set the channel destination address.
        self.dma
            .cpar4
            .write(|w| unsafe { w.bits(SDMMC1_ADDRESS + FIFO_OFFSET) });
        //    - Set the number of words to transfer.
        self.dma.cndtr4.write(|w| w.ndt().bits(0x80));

        //    - Set the word size, direction and increments.
        self.dma.ccr4.write(|w| {
            w.dir()
                .clear_bit()
                .minc()
                .set_bit()
                .pinc()
                .clear_bit()
                .msize()
                .bits32()
                .psize()
                .bits32()
        });

        //    - Enable the channel.
        self.dma.ccr4.modify(|_, w| w.en().set_bit());
        // c. Set the data control register:
        self.sdmmc.dctrl.write(|w| unsafe {
            w.dten()
                .set_bit()
                .dtdir()
                .set_bit()
                .dtmode()
                .clear_bit()
                .dmaen()
                .set_bit()
                .dblocksize()
                .bits(0x9)
        });

        // d. Set the address.
        self.sdmmc
            .arg
            .write(|w| unsafe { w.bits(address.0) });

        // e. Set the command register.
        self.sdmmc.cmd.write(|w| unsafe {
            w.cmdindex()
                .bits(Command::READ_BLOCK as u8)
                .waitresp()
                .bits(1)
                .cpsmen()
                .set_bit()
        });

        block!(self.check_command(true))?;
        self.state = State::Reading;
        Ok(())
    }

    unsafe fn write_block(&mut self, _block: &Block, _address: BlockIndex) -> nb::Result<(), Error> {
        panic!("not implemented: write_block");
    }

    fn result(&mut self) -> nb::Result<(), Error> {
        let status = self.sdmmc.sta.read();
        match self.state {
            State::Uninitialized => Err(Other(Error::Uninitialized)),
            State::Ready => panic!("called CardHost::result without starting an operation"),
            State::Reading if status.rxact().bit() => Err(WouldBlock),
            State::Writing if status.txact().bit() => Err(WouldBlock),
            State::Reading | State::Writing => Ok(()),
        }?;

        self.sdmmc.icr.write(|w| unsafe { w.bits(STATUS_ERROR_MASK) });
        self.state = State::Ready;
        if status.dcrcfail().bit() {
            Err(Other(CRCFail))
        } else if status.dtimeout().bit() {
            Err(Other(Timeout))
        } else if status.rxoverr().bit() {
            Err(Other(ReceiveOverrun))
        } else if status.txunderr().bit() {
            Err(Other(SendUnderrun))
        } else if !status.dataend().bit() || !status.dbckend().bit() {
            Err(Other(UnknownResult))
        } else {
            Ok(())
        }
    }
}
