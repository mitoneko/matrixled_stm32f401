//! matrix_ledをキャラクタディスプレイとして使用する
//! matrix_ledモジュールは、display_ledを経由して使用する。

use super::matrix_led::Matrix;
use cortex_m;
//use cortex_m_semihosting::dbg;
use stm32f4::stm32f401;

///ディスプレイドライバ
pub struct DisplayLed<'a> {
    led: Matrix<'a>,
}

impl<'a> DisplayLed<'a> {
    pub fn new(device: &'a stm32f401::Peripherals) -> Self {
        let mut display = DisplayLed {
            led: Matrix::new(&device),
        };
        display.led.clear();
        display
    }
}

use core::fmt;
use core::fmt::Write;
use misakifont::font48::FONT48;
//use misakifont::font88::FONT88;

impl fmt::Write for DisplayLed<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if s.len() == 0 {
            return Ok(());
        }
        let c1 = match s.chars().next() {
            Some(c) => c,
            None => ' ',
        };
        let font = FONT48.get_char(c1 as u8);
        self.led.draw_bitmap(0, 0, 4, font);
        while let Err(_) = self.led.flash_led() {}

        Ok(())
    }
}

pub fn print_led_fmt(disp: &mut DisplayLed, args: fmt::Arguments) {
    disp.write_fmt(args).unwrap()
}

#[macro_export]
macro_rules! print_led {
    ($disp:ident, $($arg:tt)*) => {
        display_led::print_led_fmt(&mut $disp, format_args!($($arg)*));
    }
}
