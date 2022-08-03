//!

// #![deny(
//     missing_docs,
//     missing_debug_implementations,
//     missing_copy_implementations,
//     unstable_features,
//     warnings
// )]

use super::*;
use bit_field::BitField;
use embedded_hal::blocking::i2c::{Write, WriteRead};
use num_enum::TryFromPrimitive;

/// MCP23008 Register addresses
#[allow(non_camel_case_types, clippy::upper_case_acronyms, dead_code)]
#[derive(Debug, Copy, Clone, PartialEq)]
enum Register {
    IODIR = 0x00,
    IPOL = 0x01,
    GPINTEN = 0x02,
    DEFVAL = 0x03,
    INTCON = 0x04,
    IOCON = 0x05,
    GPPU = 0x06,
    INTF = 0x07,
    INTCAP = 0x08,
    GPIO = 0x09,
    OLAT = 0x10,
}

/// GPIO pin
#[allow(missing_docs)]
#[derive(Debug, Copy, Clone, PartialEq, TryFromPrimitive)]
#[repr(u8)]
pub enum Pin {
    Gp0 = 0,
    Gp1 = 1,
    Gp2 = 2,
    Gp3 = 3,
    Gp4 = 4,
    Gp5 = 5,
    Gp6 = 6,
    Gp7 = 7,
}

/// Defines errors
#[derive(Debug, Copy, Clone)]
pub enum Error<E> {
    /// Underlying bus error
    BusError(E),
    /// Interrupt pin not found
    InterruptPinError,
}

impl<E> From<E> for Error<E> {
    fn from(error: E) -> Self {
        Error::BusError(error)
    }
}

/// MCP23017 I2C GPIO extender.
/// See the crate-level documentation for general info on the device and the operation of this
/// driver.
#[derive(Clone, Copy, Debug)]
pub struct MCP23017<I2C> {
    com: I2C,
    /// The I2C slave address of this device.
    pub address: u8,
}

impl<I2C, E> MCP23017<I2C>
where
    I2C: WriteRead<Error = E> + Write<Error = E>,
{
    /// The default I2C address of the MCP23017.
    const DEFAULT_ADDRESS: u8 = 0x20;

    /// Creates an expander with the default configuration.
    pub fn new_default(i2c: I2C) -> Result<MCP23017<I2C>, Error<E>> {
        MCP23017::new(i2c, Self::DEFAULT_ADDRESS)
    }

    /// Creates an expander with specific address.
    pub fn new(i2c: I2C, address: u8) -> Result<MCP23017<I2C>, Error<E>> {
        Ok(MCP23017 { com: i2c, address })
    }

    fn read_register(&mut self, reg: Register) -> Result<u8, E> {
        let mut data = [0u8];
        self.com.write_read(self.address, &[reg as u8], &mut data)?;
        Ok(data[0])
    }

    fn read_double_register(&mut self, reg: Register) -> Result<u16, E> {
        let mut data = [0u8; 2];
        self.com.write_read(self.address, &[reg as u8], &mut data)?;
        Ok(u16::from_le_bytes(data))
    }

    fn write_register(&mut self, reg: Register, data: u8) -> Result<(), E> {
        self.com.write(self.address, &[reg as u8, data])
    }

    fn write_double_register(&mut self, reg: Register, data: u16) -> Result<(), E> {
        let data = data.to_le_bytes();
        self.com.write(self.address, &[reg as u8, data[0], data[1]])
    }

    /// Reads a single port, A or B, and returns its current 8 bit value.
    pub fn read_gpio(&mut self, port: Port) -> Result<u8, E> {
        self.read_register(port.into())
    }

    /// Reads all 16 pins (port A and B) into a single 16 bit variable.
    pub fn read_gpioab(&mut self) -> Result<u16, E> {
        self.read_double_register(Register::GPIOA)
    }

    /// Writes all the pins of one port with the value at the same time.
    pub fn write_gpio(&mut self, port: Port, value: u8) -> Result<(), E> {
        self.write_register(port.into(), value)
    }

    /// Writes all the pins with the value at the same time.
    pub fn write_gpioab(&mut self, value: u16) -> Result<(), E> {
        self.write_double_register(Register::GPIOA, value)
    }

    /// Sets all pins' modes to either `Mode::Input` or `Mode::Output`.
    pub fn set_mode_all(&mut self, mode: Mode) -> Result<(), E> {
        let data = if mode == Mode::Input { 0xffff } else { 0x0000 };
        self.write_double_register(Register::IODIRA, data)
    }

    fn map_pin(pin: Pin, rega: Register, regb: Register) -> (Register, usize) {
        let pin = pin as usize;
        (if pin < 8 { rega } else { regb }, pin & 7)
    }

    fn bit(&mut self, pin: Pin, a_reg: Register, b_reg: Register) -> Result<bool, E> {
        let (reg, pin) = Self::map_pin(pin, a_reg, b_reg);
        let data = self.read_register(reg)?;
        Ok(data.get_bit(pin))
    }

    /// Updates a single bit in the register associated with the given pin.
    /// This will read the register (`port_a_reg` for pins 0-7, `port_b_reg` for the other eight),
    /// set the bit (as specified by the pin position within the register), and write the register
    /// back to the device.
    fn set_bit(
        &mut self,
        pin: Pin,
        value: bool,
        a_reg: Register,
        b_reg: Register,
    ) -> Result<(), E> {
        let (reg, pin) = Self::map_pin(pin, a_reg, b_reg);
        let mut data = self.read_register(reg)?;
        data.set_bit(pin, value);
        self.write_register(reg, data)
    }

    /// Sets the mode for a single pin to either `Mode::Input` or `Mode::Output`.
    pub fn set_mode(&mut self, pin: Pin, mode: Mode) -> Result<(), E> {
        self.set_bit(pin, mode == Mode::Input, Register::IODIRA, Register::IODIRB)
    }

    /// Writes a single bit to a single pin (GPIOA/GPIOB).
    /// This function accesses the GPIOA/GPIOB registers.
    pub fn write_pin(&mut self, pin: Pin, value: Level) -> Result<(), E> {
        self.set_bit(pin, value == Level::High, Register::GPIOA, Register::GPIOB)
    }

    /// Reads a single pin (GPIOA/GPIOB).
    pub fn read_pin(&mut self, pin: Pin) -> Result<bool, E> {
        self.bit(pin, Register::GPIOA, Register::GPIOB)
    }

    /// Writes a single bit to a single pin (OLATA/OLATB).
    pub fn write_output_latch(&mut self, pin: Pin, value: Level) -> Result<(), E> {
        self.set_bit(pin, value == Level::High, Register::OLATA, Register::OLATB)
    }

    /// Enables or disables the internal pull-up resistor for a single pin (GPPUA/GPPUB).
    pub fn set_pull_up(&mut self, pin: Pin, value: PullUp) -> Result<(), E> {
        self.set_bit(
            pin,
            value == PullUp::Enabled,
            Register::GPPUA,
            Register::GPPUB,
        )
    }

    /// Inverts the input polarity for a single pin (IPOLA/IPOLB).
    pub fn set_input_polarity(&mut self, pin: Pin, value: Polarity) -> Result<(), E> {
        self.set_bit(
            pin,
            value == Polarity::Inverted,
            Register::IPOLA,
            Register::IPOLB,
        )
    }
}