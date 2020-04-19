#![no_std]
#![no_main]

// pick a panicking behavior
extern crate panic_halt; // you can put a breakpoint on `rust_begin_unwind` to catch panics
// extern crate panic_abort; // requires nightly
// extern crate panic_itm; // logs messages over ITM; requires ITM support
// extern crate panic_semihosting; // logs messages to the host stderr; requires a debugger

use cortex_m_rt::entry;
use stm32f4::stm32f401;
//use cortex_m_semihosting::dbg;

use misakifont::font88::FONT88;
use matrixled::matrix_led;

#[entry]
fn main() -> ! {
    let device = stm32f401::Peripherals::take().unwrap();

    init_clock(&device);
    gpio_setup(&device);
    spi1_setup(&device);

    device.GPIOA.bsrr.write(|w| w.bs1().set());

    let mut matrix = matrix_led::Matrix::new(&device);

    device.GPIOA.bsrr.write(|w| w.bs10().set());
    //device.GPIOA.bsrr.write(|w| w.bs11().set());

    //let font = FONT48.get_char(0x52);
    let chars=[0xc5,0xda,0xb0,0xe6,0xcd,0xa5,0xbb,0xd2];
    for c in 0..4 {
        let font = FONT88.get_char(chars[c*2],chars[c*2+1]);
        matrix.draw_bitmap((c as i32)*8, 0, 8, font);
    }
    matrix.flash_led();



    device.GPIOA.bsrr.write(|w| w.bs11().set());

    loop {
        // your code goes here
    }
}

                


/// システムクロックの初期設定
/// 　クロック周波数　48MHz
fn init_clock(device : &stm32f401::Peripherals) {

    // システムクロック　48MHz
    // PLLCFGR設定
    // hsi(16M)/8*192/8=48MHz
    {
        let pllcfgr = &device.RCC.pllcfgr;
        pllcfgr.modify(|_,w| w.pllsrc().hsi());
        pllcfgr.modify(|_,w| w.pllp().div8());
        pllcfgr.modify(|_,w| unsafe { w.plln().bits(192u16) });
        pllcfgr.modify(|_,w| unsafe { w.pllm().bits(8u8) });
    }

    // PLL起動
    device.RCC.cr.modify(|_,w| w.pllon().on());
    while device.RCC.cr.read().pllrdy().is_not_ready() {
        // PLLの安定をただひたすら待つ
    }

    // フラッシュ読み出し遅延の変更
    device.FLASH.acr.modify(|_,w| unsafe {w.latency().bits(1u8)});
    // システムクロックをPLLに切り替え
    device.RCC.cfgr.modify(|_,w| w.sw().pll());
    while !device.RCC.cfgr.read().sws().is_pll() { /*wait*/ }

    // APB2のクロックを1/16
    //device.RCC.cfgr.modify(|_,w| w.ppre2().div2());
}

/// gpioのセットアップ
fn gpio_setup(device : &stm32f401::Peripherals) {
    // GPIOA 電源
    device.RCC.ahb1enr.modify(|_,w| w.gpioaen().enabled());

    // GPIOC セットアップ
    let gpioa = &device.GPIOA;
    gpioa.moder.modify(|_,w| w.moder1().output());
    gpioa.moder.modify(|_,w| w.moder10().output());
    gpioa.moder.modify(|_,w| w.moder11().output());

    // SPI端子割付け
    gpioa.moder.modify(|_,w| w.moder7().alternate()); // SPI1_MOSI
    gpioa.afrl.modify(|_,w| w.afrl7().af5());
    gpioa.ospeedr.modify(|_,w| w.ospeedr7().very_high_speed());
    gpioa.otyper.modify(|_,w| w.ot7().push_pull()); 
    gpioa.moder.modify(|_,w| w.moder5().alternate()); // SPI1_CLK
    gpioa.afrl.modify(|_,w| w.afrl5().af5());
    gpioa.ospeedr.modify(|_,w| w.ospeedr5().very_high_speed());
    gpioa.otyper.modify(|_,w| w.ot5().push_pull());
    gpioa.moder.modify(|_,w| w.moder4().output());   // NSS(CS)
    gpioa.ospeedr.modify(|_,w| w.ospeedr4().very_high_speed());
    gpioa.otyper.modify(|_,w| w.ot4().push_pull());
}

/// SPIのセットアップ
fn spi1_setup(device : &stm32f401::Peripherals) {
    // 電源投入
    device.RCC.apb2enr.modify(|_,w| w.spi1en().enabled());

    let spi1 = &device.SPI1;
    spi1.cr1.modify(|_,w| w.bidimode().unidirectional());
    spi1.cr1.modify(|_,w| w.dff().sixteen_bit());
    spi1.cr1.modify(|_,w| w.lsbfirst().msbfirst());
    spi1.cr1.modify(|_,w| w.br().div4()); // 基準クロックは48MHz
    spi1.cr1.modify(|_,w| w.mstr().master());
//    spi1.cr1.modify(|_,w| w.cpol().idle_high());
    spi1.cr1.modify(|_,w| w.cpol().idle_low());
    spi1.cr1.modify(|_,w| w.cpha().first_edge());
//    spi1.cr1.modify(|_,w| w.cpha().second_edge());
    spi1.cr1.modify(|_,w| w.ssm().enabled());
    spi1.cr1.modify(|_,w| w.ssi().slave_not_selected());
}

/*
/// TIM11のセットアップ
fn tim11_setup(device : &stm32f401::Peripherals) {
    // TIM11 電源
    device.RCC.apb2enr.modify(|_,w| w.tim11en().enabled());

    // TIM11 セットアップ
    let tim11 = &device.TIM11;
    tim11.psc.modify(|_,w| w.psc().bits(48_000u16 - 1)); // 1ms
    tim11.arr.modify(|_,w| unsafe { w.arr().bits(500u16) }); // 500ms
}
*/
