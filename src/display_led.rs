//! matrix_ledをキャラクタディスプレイとして使用する
//! matrix_ledモジュールは、display_ledを経由して使用する。

use super::matrix_led::Matrix;
use cortex_m;
//use cortex_m_semihosting::dbg;
use stm32f4::stm32f401;

///ディスプレイドライバ
pub struct DisplayLed<'a> {
    led: Matrix<'a>,
    buff: [u8; 50],
    buff_len: usize,
}

impl<'a> DisplayLed<'a> {
    pub fn new(device: &'a stm32f401::Peripherals) -> Self {
        let mut display = DisplayLed {
            led: Matrix::new(&device),
            buff: [0; 50],
            buff_len: 0,
        };
        display.led.clear();
        display
    }

    pub fn clear(&mut self) {
        self.led.clear();
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
        let mut is_output = false;
        for c in s.chars() {
            match c {
                '\n' => {
                    is_output = true;
                }
                cc if cc.is_ascii_control() => {}
                cc => {
                    if self.buff_len < 50 {
                        self.buff[self.buff_len] = cc as u8;
                        self.buff_len += 1;
                    }
                }
            }
        }

        if is_output == true {
            self.led.clear();
            for (i, c) in self.buff[0..self.buff_len].iter().enumerate() {
                let font = FONT48.get_char(*c);
                self.led.draw_bitmap((i * 4) as i32, 0, 4, font);
                while let Err(_) = self.led.flash_led() {}
            }
            self.buff_len = 0;
        }

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
