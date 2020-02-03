//! Draw a square, circle and triangle on the screen using the `embedded_graphics` crate.
//!
//! This example is for the STM32F103 "Blue Pill" board using I2C1.
//!
//! Wiring connections are as follows for a CRIUS-branded display:
//!
//! ```
//!      Display -> Blue Pill
//! (black)  GND -> GND
//! (red)    +5V -> VCC
//! (yellow) SDA -> PB9
//! (green)  SCL -> PB8
//! ```
//!
//! Run on a Blue Pill with `cargo run --example graphics_i2c`.

#![no_std]
#![no_main]

extern crate cortex_m;
extern crate cortex_m_rt as rt;
extern crate panic_semihosting;
extern crate stm32f1xx_hal as hal;

use cortex_m_rt::ExceptionFrame;
use cortex_m_rt::{entry, exception};
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Circle, Rectangle, Triangle};
use hal::i2c::{BlockingI2c, DutyCycle, Mode};
use hal::prelude::*;
use hal::stm32;
use ssd1306::prelude::*;
use ssd1306::Builder;

#[entry]
fn main() -> ! {
    let dp = stm32::Peripherals::take().unwrap();

    let mut flash = dp.FLASH.constrain();
    let mut rcc = dp.RCC.constrain();

    let clocks = rcc.cfgr.freeze(&mut flash.acr);

    let mut afio = dp.AFIO.constrain(&mut rcc.apb2);

    let mut gpiob = dp.GPIOB.split(&mut rcc.apb2);

    let scl = gpiob.pb8.into_alternate_open_drain(&mut gpiob.crh);
    let sda = gpiob.pb9.into_alternate_open_drain(&mut gpiob.crh);

    let i2c = BlockingI2c::i2c1(
        dp.I2C1,
        (scl, sda),
        &mut afio.mapr,
        Mode::Fast {
            frequency: 400_000.hz(),
            duty_cycle: DutyCycle::Ratio2to1,
        },
        clocks,
        &mut rcc.apb1,
        1000,
        10,
        1000,
        1000,
    );

    let mut disp: GraphicsMode<_> = Builder::new()
        .size(DisplaySize::Display72x40)
        .connect_i2c(i2c)
        .into();

    disp.init().unwrap();

    let size = 10;
    let offset = Point::new(10, (42 / 2) - (size / 2) - 1);
    let spacing = size + 10;

    // screen outline
    // default display size is 128x64 if you don't pass a _DisplaySize_
    // enum to the _Builder_ struct
    disp.draw(
        Rectangle::new(Point::new(0, 0), Point::new(71, 39))
            .stroke(Some(BinaryColor::On))
            .into_iter(),
    );

    // Triangle
    disp.draw(
        Triangle::new(
            Point::new(0, size),
            Point::new(size / 2, 0),
            Point::new(size, size),
        )
        .translate(offset)
        .stroke(Some(BinaryColor::On))
        .into_iter(),
    );

    // Move over to next position
    let offset = offset + Point::new(spacing, 0);

    // Draw a square
    disp.draw(
        Rectangle::new(Point::new(0, 0), Point::new(size, size))
            .translate(offset)
            .stroke(Some(BinaryColor::On))
            .into_iter(),
    );

    // Move over a bit more
    let offset = offset + Point::new(spacing, 0);

    // Circle
    disp.draw(
        Circle::new(Point::new(size / 2, size / 2), size as u32 / 2)
            .translate(offset)
            .stroke(Some(BinaryColor::On))
            .into_iter(),
    );

    disp.flush().unwrap();

    loop {}
}

#[exception]
fn HardFault(ef: &ExceptionFrame) -> ! {
    panic!("{:#?}", ef);
}
