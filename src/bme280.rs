use stm32f1xx_hal::{
    i2c::{BlockingI2c, Mode},
    pac::{self, I2C1, I2C2}, 
    afio::Parts,
    rcc::Clocks,
    prelude::*, gpio::OpenDrain
}; // STM32F1 hardware abstraction layer crate
use cortex_m_semihosting::hprintln;
use embedded_hal::blocking::i2c::{Write, Read, WriteRead};
use core::fmt::Debug;

// If you write the MSB, it will send back the MSB followed by the LSB...??
// https://dev.to/apollolabsbin/stm32f4-embedded-rust-at-the-hal-i2c-temperature-pressure-sensing-with-bmp180-578l

pub const BME280_CHIP_ID: u8 = 0x60; // The knockoff BME280 has the BME180 id which is 0x60
pub const BME280_ADDR: u8 = 0x76; // I2C address of the chip
pub const REG_ID_ADDR: u8 = 0xD0; // chip ID number - should be 0x58
pub const _REG_RESET_ADDR: u8 = 0xE0; // Writing 0xB6 to this register will power on reset the device
pub const _REG_STATUS_ADDR: u8 = 0xF3; // status contains two bits which indicates status (image update or measuring statuses)
pub const REG_CTRL_MEAS_ADDR: u8 = 0xF4; // Controls oversampling of the temp data and power mode
pub const _REG_CONFIG_ADDR: u8 = 0xF5; // Sets rate, filter and interface options of the device
pub const REG_PRESS_ADDR: u8 = 0xF7; // pressure measurement: _msb, _lsb, _xlsb (depending on resolution)
    // note that not all bits of xlsb is used - only up to 4 extra bits are used based on resolution
pub const REG_TEMP_ADDR: u8 = 0xFA; // temperature measurement:  _msb, _lsb, _xlsb (depending on resolution)
     // note that not all bits of xlsb is used - only up to 4 extra bits are used based on resolution
pub const REG_DIG_T1_ADDR: u8 = 0x88;
pub const REG_DIG_T2_ADDR: u8 = 0x8A; 
pub const REG_DIG_T3_ADDR: u8 = 0x8C; 
pub const REG_DIG_P1_ADDR: u8 = 0x8E; 
pub const REG_DIG_P2_ADDR: u8 = 0x90; 
pub const REG_DIG_P3_ADDR: u8 = 0x92; 
pub const REG_DIG_P4_ADDR: u8 = 0x94; 
pub const REG_DIG_P5_ADDR: u8 = 0x96; 
pub const REG_DIG_P6_ADDR: u8 = 0x98; 
pub const REG_DIG_P7_ADDR: u8 = 0x9A; 
pub const REG_DIG_P8_ADDR: u8 = 0x9C;
pub const REG_DIG_P9_ADDR: u8 = 0x9E; 
pub const OSRS_P_MASK: u8 = 0b00011100;
pub const OSRS_T_MASK: u8 = 0b11100000;

// Architecture 1: use traits, multiple Bme280 structs
// trait Bme280 {
//     fn new() -> Self;
//     fn read_u8() -> u8;
//     fn read_u16() -> u16;
// }

// then after defining trait, implement these:
// struct Bme280_pins67 which implements trait Bme280
// struct Bme280_pins89 which implements trait Bme280
// struct Bme280_pins1011 which implements trait Bme280
// but the issue is that there are many repeated functions - only new() is unique

// can we not instead make a Bme280 generic type?
// new() function does not allow us to 

// Architecture 2: Generic struct
// Generic struct - pass in I2C instead of creating it inside here - too difficult
// and not much benefit to create i2c object in this module
pub struct Bme280<I2cT> {
    pub i2c_addr: u8,
    i2c: I2cT,
    pub config: Bme280Configs
}

pub struct Bme280Configs {
    pub chip_id: u8,
    pub dig_t1: u16,
    pub dig_t2: i16,
    pub dig_t3: i16,
    pub dig_p1: u16,
    pub dig_p2: i16,
    pub dig_p3: i16,
    pub dig_p4: i16,
    pub dig_p5: i16,
    pub dig_p6: i16,
    pub dig_p7: i16,
    pub dig_p8: i16,
    pub dig_p9: i16,
    pub osrs_p: Bme280Resolution,
    pub osrs_t: Bme280Resolution
}

impl Default for Bme280Configs {
    fn default() -> Self {
        Bme280Configs {
            chip_id: 0,
            dig_t1: 0,
            dig_t2: 0,
            dig_t3: 0,
            dig_p1: 0,
            dig_p2: 0,
            dig_p3: 0,
            dig_p4: 0,
            dig_p5: 0,
            dig_p6: 0,
            dig_p7: 0,
            dig_p8: 0,
            dig_p9: 0,
            osrs_p: Bme280Resolution::Skip,
            osrs_t: Bme280Resolution::Skip
        }
    }
}

pub struct Bme280Control {
    temp_oversamp: bool, // oversampling gives higher resolution - osrs_t bits in control register 0xF4
    pres_oversamp: bool  // oversampling gives higher resolution - osrs_p bits in control register 0xF4
}

#[repr(u8)] // allow us to cast enums to u16s
pub enum Bme280Resolution {
    Skip = 0,
    UltraLowPower = 1,
    LowPower = 2,
    StandardRes = 3,
    HighRes = 4,
    UltraHighRes = 5
}

impl Bme280Resolution {
    pub fn reverse(val: u8) -> Self {
        match val {
            0 => Bme280Resolution::Skip,
            1 => Bme280Resolution::UltraLowPower,
            2 => Bme280Resolution::LowPower,
            3 => Bme280Resolution::StandardRes,
            4 => Bme280Resolution::HighRes,
            _ => Bme280Resolution::UltraHighRes
        }
    }
}

// Specific implementation
impl<I2cT> Bme280<I2cT>
where 
    <I2cT as WriteRead>::Error: Debug,
    <I2cT as Write>::Error: Debug,
    <I2cT as Read>::Error: Debug,
    I2cT: embedded_hal::prelude::_embedded_hal_blocking_i2c_WriteRead + 
        embedded_hal::prelude::_embedded_hal_blocking_i2c_Read +
        embedded_hal::prelude::_embedded_hal_blocking_i2c_Write
{
    pub fn new(i2c: I2cT) -> Self {
        Bme280 {
            i2c_addr: BME280_ADDR,
            i2c: i2c,
            config: Bme280Configs::default()
        }   
    }

    pub fn init(&mut self, temp_res: Bme280Resolution, pres_res: Bme280Resolution) {
        self.write_temp_res(temp_res);
        self.write_pres_res(pres_res);
    }

    pub fn read_configs(&mut self) {
        self.config = Bme280Configs {
            chip_id: self.read_u8(REG_ID_ADDR),
            dig_t1: self.read_u16(REG_DIG_T1_ADDR),
            dig_t2: self.read_i16(REG_DIG_T2_ADDR),
            dig_t3: self.read_i16(REG_DIG_T3_ADDR),
            dig_p1: self.read_u16(REG_DIG_P1_ADDR),
            dig_p2: self.read_i16(REG_DIG_P2_ADDR),
            dig_p3: self.read_i16(REG_DIG_P3_ADDR),
            dig_p4: self.read_i16(REG_DIG_P4_ADDR),
            dig_p5: self.read_i16(REG_DIG_P5_ADDR),
            dig_p6: self.read_i16(REG_DIG_P6_ADDR),
            dig_p7: self.read_i16(REG_DIG_P7_ADDR),
            dig_p8: self.read_i16(REG_DIG_P8_ADDR),
            dig_p9: self.read_i16(REG_DIG_P9_ADDR),
            osrs_p: self.read_pres_res(),
            osrs_t: self.read_temp_res()
        };
        if self.config.chip_id != BME280_CHIP_ID {
            panic!("Actual chip id: {}, expected chip id: {}", self.config.chip_id, BME280_CHIP_ID);
        }
    }

    pub fn read_pres_res(&mut self) -> Bme280Resolution {
        let mut rx_buffer: [u8; 1] = [0];
        self.i2c.write_read(self.i2c_addr, &[REG_CTRL_MEAS_ADDR], &mut rx_buffer).unwrap();
        hprintln!("rx_buffer[0]: {}", rx_buffer[0]);
        Bme280Resolution::reverse((rx_buffer[0] & OSRS_P_MASK) >> 2 as u8)
    }

    pub fn read_temp_res(&mut self) -> Bme280Resolution {
        let mut rx_buffer: [u8; 1] = [0];
        self.i2c.write_read(self.i2c_addr, &[REG_CTRL_MEAS_ADDR], &mut rx_buffer).unwrap();
        hprintln!("rx_buffer[0]: {}", rx_buffer[0]);
        Bme280Resolution::reverse((rx_buffer[0] & OSRS_T_MASK) >> 5 as u8)
    }

    pub fn write_temp_res(&mut self, temp_res: Bme280Resolution) {
        let temp_res = temp_res as u8;
        let mut rx_buffer: [u8; 1] = [0];
        self.i2c.write_read(self.i2c_addr, &[REG_CTRL_MEAS_ADDR], &mut rx_buffer).unwrap();
        let existing_vals = !OSRS_T_MASK & rx_buffer[0];
        let write_val = (temp_res << 5) & existing_vals;
        self.write_u8(REG_CTRL_MEAS_ADDR, write_val);
    }

    pub fn write_pres_res(&mut self, pres_res: Bme280Resolution) {
        let pres_res = pres_res as u8;
        let mut rx_buffer: [u8; 1] = [0];
        self.i2c.write_read(self.i2c_addr, &[REG_CTRL_MEAS_ADDR], &mut rx_buffer).unwrap();
        let existing_vals = !OSRS_P_MASK & rx_buffer[0];
        let write_val = (pres_res << 2) & existing_vals;
        self.write_u8(REG_CTRL_MEAS_ADDR, write_val);
    }

    pub fn write_u8(&mut self, addr: u8, value: u8) {
        let mut rx_buffer: [u8; 2] = [addr, value];
        self.i2c.write(self.i2c_addr, &mut rx_buffer).unwrap();
    }

    pub fn read_i8(&mut self, addr: u8) -> i8 {
        let mut rx_buffer: [u8; 1] = [0];
        self.i2c.write_read(self.i2c_addr, &[addr], &mut rx_buffer).unwrap();
        return rx_buffer[0] as i8;
    }

    pub fn read_u8(&mut self, addr: u8) -> u8 {
        let mut rx_buffer: [u8; 1] = [0];
        self.i2c.write_read(self.i2c_addr, &[addr], &mut rx_buffer).unwrap();
        return rx_buffer[0];
    }

    // i2c.read: If we use a buffer of length 2, it will read the addresses starting at [addr]
    // eg. i2c.read(self.i2c_addr, &[0x88], &mut rx_buffer)
    // will return value of reg 0x88 in rx_buffer[0], value of reg 0x89 in rx_buffer[1]
    pub fn read_i16(&mut self, addr: u8) -> i16 {
        let mut rx_buffer: [u8; 2] = [0; 2];
        self.i2c.write_read(self.i2c_addr, &[addr], &mut rx_buffer).unwrap();
        return i16::from_le_bytes(rx_buffer);
    }

    pub fn read_u16(&mut self, addr: u8) -> u16 {
        let mut rx_buffer: [u8; 2] = [0; 2];
        self.i2c.write_read(self.i2c_addr, &[addr], &mut rx_buffer).unwrap();
        // hprintln!("rx_buffer[0]: {}", rx_buffer[0]);
        // hprintln!("rx_buffer[1]: {}", rx_buffer[1]);
        return u16::from_le_bytes(rx_buffer);
    }

    pub fn read_dig_t1(&mut self) -> u16 {
        let mut rx_buffer: [u8; 1] = [0; 1];
        self.i2c.write_read(self.i2c_addr, &[0x88], &mut rx_buffer).unwrap(); //lsb
        let d1 = rx_buffer[0];
        //hprintln!("read dig_t1: rx_buffer[0]: {}", rx_buffer[0]);
        self.i2c.write_read(self.i2c_addr, &[0x89], &mut rx_buffer).unwrap(); //msb
        let d2 = rx_buffer[0];
        //hprintln!("read dig_t1: rx_buffer[0]: {}", rx_buffer[0]);
        let mut rx_buffer_2: [u8; 2] = [0; 2];
        rx_buffer_2[0] = d1; //lsb
        rx_buffer_2[1] = d2; //msb
        u16::from_le_bytes(rx_buffer_2)
    }

    // pub fn read_temp_adc(&mut self) -> i32 {
    //     let mut rx_buffer: [u8; 2] = [0; 2];
    //     self.i2c.write_read(self.i2c_addr, &[REG_TEMP_ADDR], &mut rx_buffer).unwrap();
    //     let t1 = rx_buffer[0];
    //     self.i2c.write_read(self.i2c_addr, &[REG_DIG_T1_ADDR], &mut rx_buffer).unwrap();
    //     let t1 = rx_buffer[0];
    //     self.i2c.write_read(self.i2c_addr, &[REG_DIG_T1_ADDR], &mut rx_buffer).unwrap();
    // }

    // pub fn read_temperature(&mut self) -> f32 {

    //     self.compensate_t_double(adc_temp, self.config.dig_t1, self.config.dig_t2, self.config.dig_t3);
    // }

    fn compensate_t_double(&self, adc_temp: i32, dig_t1: u8, dig_t2: i8, dig_t3: i8) -> i32{
        let var1 = ( ( ((adc_temp>>3) - ((dig_t1 as i32)<<1)) ) * (dig_t2 as i32)) >> 11;
        let var2 = ((( ((adc_temp>>4) - (dig_t2 as i32)) * ( (adc_temp>>4) - (dig_t1 as i32) )) >> 12) *
            (dig_t3 as i32)) >> 14;
        let t_fine = var1 + var2;
        return (t_fine * 5 + 128) >> 8;
    }


}
